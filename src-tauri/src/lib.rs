mod openai;
mod db;
mod embed_sidecar;
mod ingest;
mod memory;
mod pdf_render;
mod selection_map;

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use ingest::IngestState;
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

use std::collections::HashMap;

pub struct DbState(pub Arc<Mutex<rusqlite::Connection>>);

pub struct PathsState {
    pub db_path: PathBuf,
}

fn emit_agent_status(app: &tauri::AppHandle, agent: &str, status: &str, detail: &str) {
    app.emit(
        "agent-status",
        serde_json::json!({
            "agent": agent,
            "status": status,
            "detail": detail,
        }),
    )
    .ok();
}

fn run_rag_hybrid(
    fts_ids: Vec<i64>,
    rows: &[db::PageRowLite],
    qvec: &[f32],
    k: f64,
    limit: usize,
) -> Vec<i32> {
    let mut ranks_vecs: Vec<HashMap<i64, usize>> = vec![];
    if !fts_ids.is_empty() {
        ranks_vecs.push(memory::ranks_from_ordered_ids(&fts_ids));
    }
    let mut vec_scores: Vec<(i64, f32)> = vec![];
    for r in rows {
        if let Some(ref b) = r.embedding {
            if b.len() >= 16 {
                if let Some(dv) = memory::f32_blob_to_vec(b) {
                    let sim = memory::cosine_dot_norm_q_d(qvec, &dv);
                    vec_scores.push((r.id, sim));
                }
            }
        }
    }
    if !vec_scores.is_empty() {
        ranks_vecs.push(memory::ranks_from_vector_scores(&vec_scores));
    }
    if ranks_vecs.is_empty() {
        return vec![];
    }
    let fused = memory::reciprocal_rank_fusion(&ranks_vecs, k);
    // id -> page_num のマップ
    let id_to_page: HashMap<i64, i32> = rows.iter().map(|r| (r.id, r.page_num)).collect();
    let mut result: Vec<i32> = fused
        .iter()
        .take(limit)
        .filter_map(|(id, _)| id_to_page.get(id).cloned())
        .collect();
    result.dedup();
    result
}

fn run_memory_hybrid(
    fts_ids: Vec<i64>,
    rows: &[db::MemoryRowLite],
    qvec: &[f32],
    now: i64,
    k: f64,
    lambda: f64,
    limit: usize,
) -> Vec<serde_json::Value> {
    let mut ranks_vecs: Vec<HashMap<i64, usize>> = vec![];
    if !fts_ids.is_empty() {
        ranks_vecs.push(memory::ranks_from_ordered_ids(&fts_ids));
    }
    let mut vec_scores: Vec<(i64, f32)> = vec![];
    for r in rows {
        if let Some(ref b) = r.embedding {
            if b.len() >= 16 {
                if let Some(dv) = memory::f32_blob_to_vec(b) {
                    let sim = memory::cosine_dot_norm_q_d(qvec, &dv);
                    vec_scores.push((r.id, sim));
                }
            }
        }
    }
    if !vec_scores.is_empty() {
        ranks_vecs.push(memory::ranks_from_vector_scores(&vec_scores));
    }
    if ranks_vecs.is_empty() {
        return vec![];
    }
    let fused = memory::reciprocal_rank_fusion(&ranks_vecs, k);
    let mut created = HashMap::new();
    let mut decay_w = HashMap::new();
    let mut meta: HashMap<i64, (String, String)> = HashMap::new();
    for r in rows {
        created.insert(r.id, r.created_at);
        decay_w.insert(r.id, r.decay_weight);
        meta.insert(r.id, (r.session_id.clone(), r.summary.clone()));
    }
    let scored = memory::apply_decay_to_scores(fused, &created, &decay_w, now, lambda);
    let mut out = vec![];
    for (id, sc) in scored.iter().take(limit) {
        if let Some((sid, summ)) = meta.get(id) {
            out.push(serde_json::json!({
                "id": id,
                "sessionId": sid,
                "summary": summ,
                "score": sc,
            }));
        }
    }
    out
}

fn default_data_dir() -> Result<PathBuf, String> {
    let base = dirs::data_local_dir().ok_or_else(|| "data_local_dir not found".to_string())?;
    let dir = base.join("com.hikaru.math-teacher");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

#[tauri::command]
fn get_paths(state: tauri::State<'_, PathsState>) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "dbPath": state.db_path.to_string_lossy(),
    }))
}

/// `blocking_pick_file` はメインスレッドではデッドロックしうるため、blocking スレッドで実行する。
#[tauri::command]
async fn pick_pdf(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let app = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("PDF", &["pdf"])
            .blocking_pick_file()
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(picked.map(|p| p.to_string()))
}

#[tauri::command]
fn upsert_page_latex(
    book_id: String,
    page_num: i32,
    latex: String,
    state: tauri::State<'_, DbState>,
) -> Result<i64, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    db::upsert_page_latex(&conn, &book_id, page_num, &latex).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_book_pages(
    book_id: String,
    state: tauri::State<'_, DbState>,
) -> Result<Vec<serde_json::Value>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let rows = db::list_pages_preview(&conn, &book_id).map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|(page_num, preview, has_embedding)| {
            serde_json::json!({
                "pageNum": page_num,
                "preview": preview,
                "hasEmbedding": has_embedding,
            })
        })
        .collect())
}

#[tauri::command]
fn map_selection_to_latex(
    book_id: String,
    page_num: i32,
    selection_text: String,
    state: tauri::State<'_, DbState>,
) -> Result<Option<String>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let latex = db::get_page_latex(&conn, &book_id, page_num).map_err(|e| e.to_string())?;
    let Some(l) = latex.filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    Ok(selection_map::map_selection_to_excerpt(&l, &selection_text, 500))
}

#[tauri::command]
fn sample_linear_algebra_pdf() -> Result<String, String> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "manifest parent".to_string())?
        .join("text_linear_algebra.pdf");
    if !p.is_file() {
        return Err(format!(
            "リポジトリ直下に text_linear_algebra.pdf がありません: {:?}",
            p
        ));
    }
    p.canonicalize()
        .map(|c| c.to_string_lossy().to_string())
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn embed_book_pages(
    app: tauri::AppHandle,
    db: tauri::State<'_, DbState>,
    book_id: String,
) -> Result<usize, String> {
    let script = embed_sidecar::embedder_script_path()?;
    let db_arc = db.0.clone();
    let app2 = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        embed_sidecar::embed_all_missing(&db_arc, &book_id, &script, &app2)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn start_pdf_ingest(
    app: tauri::AppHandle,
    db: tauri::State<'_, DbState>,
    ingest: tauri::State<'_, IngestState>,
    book_id: String,
    pdf_path: String,
) -> Result<(), String> {
    let path = PathBuf::from(&pdf_path);
    if !path.is_file() {
        return Err(format!("PDF が見つかりません: {pdf_path}"));
    }
    if ingest.busy.swap(true, Ordering::SeqCst) {
        return Err("既に取り込みが実行中です".into());
    }
    ingest.cancel.store(false, Ordering::SeqCst);

    let app2 = app.clone();
    let db_arc = db.0.clone();
    let ctrl = IngestState {
        cancel: ingest.cancel.clone(),
        busy: ingest.busy.clone(),
    };

    tauri::async_runtime::spawn(async move {
        ingest::run_ingestion(app2, db_arc, ctrl, book_id, path).await;
    });
    Ok(())
}

#[tauri::command]
fn cancel_pdf_ingest(ingest: tauri::State<'_, IngestState>) {
    ingest.cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
fn create_session(
    book_id: String,
    page_num: Option<i32>,
    selection_text: Option<String>,
    selection_latex: Option<String>,
    parent_id: Option<String>,
    state: tauri::State<'_, DbState>,
) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO sessions (id, book_id, page_num, selection_text, selection_latex, parent_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            id,
            book_id,
            page_num,
            selection_text,
            selection_latex,
            parent_id,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(id)
}

#[tauri::command]
fn list_session_messages(
    session_id: String,
    state: tauri::State<'_, DbState>,
) -> Result<Vec<serde_json::Value>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    db::list_messages_json(&conn, &session_id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn send_session_message(
    app: tauri::AppHandle,
    db: tauri::State<'_, DbState>,
    session_id: String,
    user_text: String,
    thinking_enabled: bool,
) -> Result<String, String> {
    let trimmed = user_text.trim().to_string();
    if trimmed.is_empty() {
        return Err("空のメッセージは送れません".into());
    }

    let api_key = match std::env::var("OPENAI_API_KEY") {
        Ok(k) if !k.is_empty() => k,
        _ => return Err("OPENAI_API_KEY が空です".into()),
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;

    let mini_model = std::env::var("MATH_TEACHER_CONCEPT_MODEL")
        .unwrap_or_else(|_| "gpt-4o-mini".to_string());
    let main_model = std::env::var("MATH_TEACHER_MAIN_MODEL")
        .unwrap_or_else(|_| "gpt-4o".to_string());
    let fast_model = std::env::var("MATH_TEACHER_FAST_MODEL")
        .unwrap_or_else(|_| "gpt-4o-mini".to_string());
    let model = if thinking_enabled { main_model } else { fast_model };
    let k = std::env::var("MATH_TEACHER_RRF_K")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(60.0);
    let lambda = std::env::var("MATH_TEACHER_MEMORY_DECAY_LAMBDA")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.05);

    // ── Step 1: 入力正規化 ──
    emit_agent_status(&app, "normalizer", "running", "入力を正規化中");
    let normalized = openai::normalize_math_input(&client, &api_key, &mini_model, &trimmed)
        .await
        .unwrap_or_else(|_| trimmed.clone());
    emit_agent_status(&app, "normalizer", "done", "");

    // ── Step 2: セッション情報取得 ──
    let (sel_latex, sel_text, book_id, mut history) = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        let s = db::get_session(&conn, &session_id).map_err(|e| e.to_string())?;
        let h = db::list_message_pairs(&conn, &session_id).map_err(|e| e.to_string())?;
        (s.selection_latex, s.selection_text, s.book_id, h)
    };
    let is_selection_mode = sel_latex.as_deref().map(|s| !s.trim().is_empty()).unwrap_or(false);

    // ── Step 3: RAG Agent（自由質問モード時のみ）──
    let mut rag_context = String::new();
    if !is_selection_mode {
        emit_agent_status(&app, "rag", "running", "ページ検索中");
        let script = embed_sidecar::embedder_script_path()?;
        let norm_for_embed = normalized.clone();
        let script_for_embed = script.clone();
        let qvec_bytes = tauri::async_runtime::spawn_blocking(move || {
            embed_sidecar::run_embedder_query(&script_for_embed, &norm_for_embed)
        })
        .await
        .map_err(|e| e.to_string())
        .unwrap_or_else(|_| Err("embed failed".into()));

        if let Ok(qvec_bytes) = qvec_bytes {
            if let Some(qvec) = memory::f32_blob_to_vec(&qvec_bytes) {
                let fts_expr = db::fts5_or_terms(&normalized);
                let (fts_ids, page_rows) = {
                    let conn = db.0.lock().map_err(|e| e.to_string())?;
                    let fts = if fts_expr.is_empty() {
                        vec![]
                    } else {
                        db::pages_fts_ranked_ids(&conn, &book_id, &fts_expr, 10)
                            .unwrap_or_default()
                    };
                    let rows = db::list_page_rows_for_rag(&conn, &book_id).unwrap_or_default();
                    (fts, rows)
                };

                let top_page_nums = run_rag_hybrid(fts_ids, &page_rows, &qvec, k, 5);

                // ページLaTeX取得
                let page_latexes = {
                    let conn = db.0.lock().map_err(|e| e.to_string())?;
                    db::get_pages_by_nums(&conn, &book_id, &top_page_nums).unwrap_or_default()
                };
                for (pn, latex) in &page_latexes {
                    rag_context.push_str(&format!("=== ページ{pn} ===\n{latex}\n\n"));
                }

                // GraphRAG展開
                let concept_ids: Vec<i64> = {
                    let conn = db.0.lock().map_err(|e| e.to_string())?;
                    let concepts =
                        db::concepts_by_page_nums(&conn, &book_id, &top_page_nums)
                            .unwrap_or_default();
                    concepts.into_iter().map(|c| c.id).collect()
                };
                if !concept_ids.is_empty() {
                    let expanded_ids = {
                        let conn = db.0.lock().map_err(|e| e.to_string())?;
                        db::concept_deps_expand(&conn, &concept_ids, 1).unwrap_or_default()
                    };
                    // concept_ids に含まれない新規IDのみ追加
                    let new_ids: Vec<i64> = expanded_ids
                        .into_iter()
                        .filter(|id| !concept_ids.contains(id))
                        .collect();
                    if !new_ids.is_empty() {
                        let extra_concepts = {
                            let conn = db.0.lock().map_err(|e| e.to_string())?;
                            db::get_concepts_by_ids(&conn, &new_ids).unwrap_or_default()
                        };
                        if !extra_concepts.is_empty() {
                            rag_context.push_str("=== 関連概念（GraphRAG） ===\n");
                            for c in &extra_concepts {
                                let label = c.label.as_deref().unwrap_or("");
                                let name = c.name.as_deref().unwrap_or("");
                                rag_context.push_str(&format!(
                                    "[{} {}{}] {}\n",
                                    c.kind,
                                    label,
                                    if name.is_empty() { "".to_string() } else { format!(" {name}") },
                                    c.latex
                                ));
                            }
                        }
                    }
                }

                let page_count = top_page_nums.len();
                emit_agent_status(&app, "rag", "done", &format!("{page_count}ページ取得"));
            }
        } else {
            emit_agent_status(&app, "rag", "error", "埋め込み失敗（フォールバック）");
        }
    }

    // ── Step 4: Memory Agent ──
    emit_agent_status(&app, "memory", "running", "過去の解説を検索中");
    let memory_context: Option<String> = {
        let script = embed_sidecar::embedder_script_path().ok();
        let fts_expr = db::fts5_or_terms(&normalized);
        if let (Some(script), false) = (script, fts_expr.is_empty()) {
            let norm_for_mem = normalized.clone();
            let script_for_mem = script.clone();
            let qvec_result = tauri::async_runtime::spawn_blocking(move || {
                embed_sidecar::run_embedder_query(&script_for_mem, &norm_for_mem)
            })
            .await
            .ok()
            .and_then(|r| r.ok());

            if let Some(qvec_bytes) = qvec_result {
                if let Some(qvec) = memory::f32_blob_to_vec(&qvec_bytes) {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    let (fts_ids, rows) = {
                        let conn = db.0.lock().map_err(|e| e.to_string())?;
                        let fts = db::memory_fts_ranked_ids(&conn, &book_id, &fts_expr, 40)
                            .unwrap_or_default();
                        let rows =
                            db::list_memory_rows_book(&conn, &book_id).unwrap_or_default();
                        (fts, rows)
                    };
                    let mem_results =
                        run_memory_hybrid(fts_ids, &rows, &qvec, now, k, lambda, 3);
                    if !mem_results.is_empty() {
                        let summaries: Vec<String> = mem_results
                            .iter()
                            .filter_map(|v| v["summary"].as_str().map(|s| format!("- {s}")))
                            .collect();
                        let count = summaries.len();
                        emit_agent_status(
                            &app,
                            "memory",
                            "done",
                            &format!("{count}件の過去解説を取得"),
                        );
                        Some(summaries.join("\n"))
                    } else {
                        emit_agent_status(&app, "memory", "done", "関連解説なし");
                        None
                    }
                } else {
                    emit_agent_status(&app, "memory", "done", "ベクトル変換失敗");
                    None
                }
            } else {
                emit_agent_status(&app, "memory", "done", "埋め込み失敗");
                None
            }
        } else {
            emit_agent_status(&app, "memory", "done", "スキップ");
            None
        }
    };

    // ── Step 5: コンテキスト圧縮チェック ──
    let total_tokens: usize = history
        .iter()
        .map(|(_, c)| db::estimate_message_tokens(c))
        .sum();
    if total_tokens > 6000 {
        emit_agent_status(&app, "compression", "running", "古い会話を圧縮中");
        // 最新6件を除く古いメッセージを取得
        let all_with_ids = {
            let conn = db.0.lock().map_err(|e| e.to_string())?;
            db::list_message_pairs_with_ids(&conn, &session_id).unwrap_or_default()
        };
        let keep_count = 6usize;
        if all_with_ids.len() > keep_count {
            let to_compress = &all_with_ids[..all_with_ids.len() - keep_count];
            let old_pairs: Vec<(String, String)> = to_compress
                .iter()
                .map(|(_, role, content)| (role.clone(), content.clone()))
                .collect();
            let ids_to_compress: Vec<i64> = to_compress.iter().map(|(id, _, _)| *id).collect();

            if let Ok(summary) =
                openai::compress_old_messages(&client, &api_key, &mini_model, &old_pairs).await
            {
                {
                    let conn = db.0.lock().map_err(|e| e.to_string())?;
                    db::compress_messages(&conn, &session_id, &summary, &ids_to_compress)
                        .unwrap_or(());
                }
                // historyを圧縮済み版に更新
                let recent_pairs: Vec<(String, String)> = all_with_ids
                    [all_with_ids.len() - keep_count..]
                    .iter()
                    .map(|(_, role, content)| (role.clone(), content.clone()))
                    .collect();
                history = std::iter::once((
                    "assistant".to_string(),
                    format!("【会話圧縮サマリー】\n{summary}"),
                ))
                .chain(recent_pairs)
                .collect();
                app.emit(
                    "compression-done",
                    serde_json::json!({"sessionId": session_id}),
                )
                .ok();
            }
        }
        emit_agent_status(&app, "compression", "done", "圧縮完了");
    }

    // ── Step 6: Main Agent ──
    emit_agent_status(&app, "main_agent", "running", "回答を生成中");
    let context_latex_for_adv = if is_selection_mode {
        sel_latex.as_deref().unwrap_or("").to_string()
    } else {
        rag_context.clone()
    };
    let mut reply = openai::chat_teacher_reply_with_context(
        &client,
        &api_key,
        &model,
        if is_selection_mode { sel_latex.as_deref() } else { None },
        if !is_selection_mode && !rag_context.is_empty() { Some(rag_context.as_str()) } else { None },
        memory_context.as_deref(),
        sel_text.as_deref(),
        &history,
        &normalized,
    )
    .await?;
    emit_agent_status(&app, "main_agent", "done", "");

    // ── Step 7: Adversarial Agent（thinking ON 時のみ）──
    if thinking_enabled {
        emit_agent_status(&app, "adversarial", "running", "回答を検証中");
        let mut adversarial_detail = "問題なし";
        for _ in 0..2 {
            match openai::adversarial_check(
                &client,
                &api_key,
                &mini_model,
                &reply,
                &context_latex_for_adv,
            )
            .await
            {
                Ok((true, _)) => break,
                Ok((false, corrected)) => {
                    reply = corrected;
                    adversarial_detail = "修正あり";
                }
                Err(_) => break,
            }
        }
        emit_agent_status(&app, "adversarial", "done", adversarial_detail);
    }

    // ── Step 8: DB保存 ──
    {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        db::insert_message(&conn, &session_id, "user", &trimmed).map_err(|e| e.to_string())?;
        db::insert_message(&conn, &session_id, "assistant", &reply)
            .map_err(|e| e.to_string())?;
    }

    emit_agent_status(&app, "all", "done", "");
    Ok(reply)
}

#[tauri::command]
fn set_session_resolved_cmd(
    session_id: String,
    resolved: bool,
    state: tauri::State<'_, DbState>,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    if !resolved {
        db::delete_memory_for_session(&conn, &session_id).map_err(|e| e.to_string())?;
    }
    db::set_session_resolved(&conn, &session_id, resolved).map_err(|e| e.to_string())
}

#[tauri::command]
async fn finalize_session_memory(
    db: tauri::State<'_, DbState>,
    session_id: String,
) -> Result<serde_json::Value, String> {
    let api_key = match std::env::var("OPENAI_API_KEY") {
        Ok(k) if !k.is_empty() => k,
        _ => return Err("OPENAI_API_KEY が空です".into()),
    };
    let model = std::env::var("MATH_TEACHER_MEMORY_SUMMARY_MODEL").unwrap_or_else(|_| {
        std::env::var("MATH_TEACHER_CONCEPT_MODEL")
            .unwrap_or_else(|_| "gpt-4o-mini".to_string())
    });

    let (book_id, transcript, has_assistant) = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        let s = db::get_session(&conn, &session_id).map_err(|e| e.to_string())?;
        let pairs = db::list_message_pairs(&conn, &session_id).map_err(|e| e.to_string())?;
        let has = pairs.iter().any(|(role, _)| role == "assistant");
        let lines: Vec<String> = pairs
            .iter()
            .map(|(role, c)| format!("{role}: {c}"))
            .collect();
        (s.book_id, lines.join("\n"), has)
    };

    if !has_assistant {
        return Err("チューターとのやりとりがないため記憶に保存できません".into());
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;
    let summary =
        openai::summarize_session_memory(&client, &api_key, &model, &transcript).await?;

    let script = embed_sidecar::embedder_script_path()?;
    let summary_for_embed = summary.clone();
    let blob = tauri::async_runtime::spawn_blocking(move || {
        embed_sidecar::run_embedder(&script, &summary_for_embed)
    })
    .await
    .map_err(|e| e.to_string())??;

    {
        let mut conn = db.0.lock().map_err(|e| e.to_string())?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        db::delete_memory_for_session(&tx, &session_id).map_err(|e| e.to_string())?;
        db::insert_resolved_memory(&tx, &session_id, &book_id, &summary, &blob)
            .map_err(|e| e.to_string())?;
        db::set_session_resolved(&tx, &session_id, true).map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
    }

    let dim = (blob.len() / 4) as i64;
    Ok(serde_json::json!({
        "summary": summary,
        "dim": dim,
    }))
}

#[tauri::command]
async fn memory_search(
    db: tauri::State<'_, DbState>,
    book_id: String,
    query: String,
    limit: Option<i64>,
) -> Result<Vec<serde_json::Value>, String> {
    let fts_expr = db::fts5_or_terms(&query);
    if fts_expr.is_empty() {
        return Ok(vec![]);
    }
    let script = embed_sidecar::embedder_script_path()?;
    let q = query.trim().to_string();
    let qvec_bytes = tauri::async_runtime::spawn_blocking({
        let script = script.clone();
        move || embed_sidecar::run_embedder_query(&script, &q)
    })
    .await
    .map_err(|e| e.to_string())??;

    let qvec = memory::f32_blob_to_vec(&qvec_bytes)
        .ok_or_else(|| "埋め込みベクトルの解析に失敗しました".to_string())?;

    let limit = limit.unwrap_or(8).clamp(1, 30) as usize;
    let k = std::env::var("MATH_TEACHER_RRF_K")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(60.0);
    let lambda = std::env::var("MATH_TEACHER_MEMORY_DECAY_LAMBDA")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.05);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let (fts_ids, rows) = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        let fts = db::memory_fts_ranked_ids(&conn, &book_id, &fts_expr, 40)
            .map_err(|e| format!("全文検索エラー（キーワードを変えて試してください）: {e}"))?;
        let rows = db::list_memory_rows_book(&conn, &book_id).map_err(|e| e.to_string())?;
        (fts, rows)
    };

    Ok(run_memory_hybrid(
        fts_ids, &rows, &qvec, now, k, lambda, limit,
    ))
}

#[tauri::command]
async fn prefetch_pages(
    db: tauri::State<'_, DbState>,
    book_id: String,
    center_page: i32,
) -> Result<(), String> {
    let script = embed_sidecar::embedder_script_path()?;
    let db_arc = db.0.clone();
    tauri::async_runtime::spawn(async move {
        let page_range: Vec<i32> = ((center_page - 2)..=(center_page + 2))
            .filter(|&p| p >= 1)
            .collect();

        // 該当ページの中で embedding が未計算のものを取得
        let missing: Vec<i32> = {
            let conn = match db_arc.lock() {
                Ok(c) => c,
                Err(_) => return,
            };
            page_range
                .into_iter()
                .filter(|&p| {
                    // embedding の有無を確認
                    let has_emb: bool = conn
                        .query_row(
                            "SELECT embedding IS NOT NULL AND length(embedding) >= 16 FROM pages
                             WHERE book_id = ?1 AND page_num = ?2",
                            rusqlite::params![book_id, p],
                            |row| row.get::<_, bool>(0),
                        )
                        .unwrap_or(false);
                    !has_emb
                })
                .collect()
        };

        if missing.is_empty() {
            return;
        }

        // 各ページのLaTeXを取得して埋め込み
        for page_num in missing {
            let latex_opt = {
                let conn = match db_arc.lock() {
                    Ok(c) => c,
                    Err(_) => return,
                };
                db::get_page_latex(&conn, &book_id, page_num).ok().flatten()
            };
            let Some(latex) = latex_opt.filter(|s| !s.is_empty()) else {
                continue;
            };
            let script2 = script.clone();
            if let Ok(blob) = embed_sidecar::run_embedder(&script2, &latex) {
                let conn = match db_arc.lock() {
                    Ok(c) => c,
                    Err(_) => return,
                };
                db::update_page_embedding(&conn, &book_id, page_num, &blob).ok();
            }
        }
    });
    Ok(())
}

#[tauri::command]
fn branch_session_cmd(
    parent_id: String,
    state: tauri::State<'_, DbState>,
) -> Result<String, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    db::branch_session(&conn, &parent_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_sessions(
    book_id: String,
    state: tauri::State<'_, DbState>,
) -> Result<Vec<serde_json::Value>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, book_id, page_num, selection_text, selection_latex, parent_id, resolved, created_at
             FROM sessions WHERE book_id = ?1 ORDER BY created_at DESC LIMIT 200",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![book_id], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "bookId": row.get::<_, String>(1)?,
                "pageNum": row.get::<_, Option<i32>>(2)?,
                "selectionText": row.get::<_, Option<String>>(3)?,
                "selectionLatex": row.get::<_, Option<String>>(4)?,
                "parentId": row.get::<_, Option<String>>(5)?,
                "resolved": row.get::<_, i64>(6)?,
                "createdAt": row.get::<_, i64>(7)?,
            }))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = dotenvy::dotenv();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(IngestState::new())
        .setup(|app| {
            let data_dir = default_data_dir()?;
            let db_path = data_dir.join("math_teacher.db");
            let conn = db::open_and_migrate(&db_path).map_err(|e| e.to_string())?;
            app.manage(DbState(Arc::new(Mutex::new(conn))));
            app.manage(PathsState {
                db_path: db_path.clone(),
            });
            if cfg!(debug_assertions) {
                eprintln!("[math-teacher] DB at {}", db_path.display());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_paths,
            pick_pdf,
            upsert_page_latex,
            list_book_pages,
            map_selection_to_latex,
            sample_linear_algebra_pdf,
            embed_book_pages,
            start_pdf_ingest,
            cancel_pdf_ingest,
            create_session,
            list_sessions,
            list_session_messages,
            send_session_message,
            set_session_resolved_cmd,
            branch_session_cmd,
            finalize_session_memory,
            memory_search,
            prefetch_pages,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
