//! Python `sidecar/embedder.py` を呼び出してページ LaTeX をベクトル化し `pages.embedding` に保存。

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use base64::Engine;
use rusqlite::Connection;
use tauri::Emitter;
use tauri::AppHandle;

use crate::db;

pub fn embedder_script_path() -> Result<PathBuf, String> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "Cargo.toml 親なし".to_string())?
        .join("sidecar/embedder.py");
    if p.is_file() {
        Ok(p)
    } else {
        Err(format!("sidecar/embedder.py がありません: {:?}", p))
    }
}

fn run_embedder_payload(script: &Path, payload: serde_json::Value) -> Result<Vec<u8>, String> {
    let mut child = Command::new("python3")
        .arg(script.as_os_str())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            format!(
                "python3 を起動できません（embedder 用）。python3 と sidecar の依存を確認: {e}"
            )
        })?;

    let mut stdin = child.stdin.take().ok_or_else(|| "stdin".to_string())?;
    let payload = payload.to_string();
    stdin
        .write_all(payload.as_bytes())
        .map_err(|e| format!("embedder stdin: {e}"))?;
    drop(stdin);

    let out = child
        .wait_with_output()
        .map_err(|e| format!("embedder wait: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("embedder エラー: {err}"));
    }

    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("embedder JSON: {e}"))?;
    if let Some(err) = v.get("error").and_then(|x| x.as_str()) {
        return Err(err.to_string());
    }
    let b64 = v["b64"]
        .as_str()
        .ok_or_else(|| "embedder 応答に b64 がありません".to_string())?;
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| format!("base64: {e}"))
}

pub fn run_embedder(script: &Path, text: &str) -> Result<Vec<u8>, String> {
    run_embedder_payload(script, serde_json::json!({ "text": text }))
}

/// Memory検索・クエリ側埋め込み（E5 なら query: プレフィックス）
pub fn run_embedder_query(script: &Path, text: &str) -> Result<Vec<u8>, String> {
    run_embedder_payload(
        script,
        serde_json::json!({ "text": text, "query": true }),
    )
}

/// 型タグ（日本語）を付与した概念テキストを構築する
fn concept_embed_text(
    kind: &str,
    label: Option<&str>,
    name: Option<&str>,
    latex: &str,
    context: Option<&str>,
) -> String {
    let tag = match kind {
        "definition" => "[定義]",
        "theorem"    => "[定理]",
        "lemma"      => "[補題]",
        "example"    => "[例]",
        "proof"      => "[証明]",
        "remark"     => "[注記]",
        _            => "[その他]",
    };
    let mut header_parts: Vec<&str> = vec![tag];
    let label_s;
    let name_s;
    if let Some(l) = label { if !l.is_empty() { label_s = l.to_string(); header_parts.push(&label_s); } }
    if let Some(n) = name  { if !n.is_empty() { name_s = n.to_string(); header_parts.push(&name_s); } }
    let header = header_parts.join(" ");
    let latex_clip: String = latex.chars().take(6000).collect();
    if let Some(ctx) = context.filter(|s| !s.is_empty()) {
        format!("{header}\n{ctx}\n{latex_clip}")
    } else {
        format!("{header}\n{latex_clip}")
    }
}

/// 概念単位の埋め込みを生成（型プレフィックス + コンテキスト付き）
pub fn run_embedder_concept(
    script: &Path,
    kind: &str,
    label: Option<&str>,
    name: Option<&str>,
    latex: &str,
    context: Option<&str>,
) -> Result<Vec<u8>, String> {
    let text = concept_embed_text(kind, label, name, latex, context);
    run_embedder_payload(script, serde_json::json!({ "text": text }))
}

pub fn embed_all_missing(
    db: &Arc<Mutex<Connection>>,
    book_id: &str,
    script: &Path,
    app: &AppHandle,
) -> Result<usize, String> {
    let pages = {
        let conn = db.lock().map_err(|e| e.to_string())?;
        db::list_pages_missing_embedding(&conn, book_id).map_err(|e| e.to_string())?
    };
    let total = pages.len() as i32;
    let mut done = 0usize;
    for (i, page_num) in pages.iter().enumerate() {
        let latex = {
            let conn = db.lock().map_err(|e| e.to_string())?;
            db::get_page_latex(&conn, book_id, *page_num)
                .map_err(|e| e.to_string())?
                .filter(|s| !s.is_empty())
                .ok_or_else(|| format!("ページ {page_num} に LaTeX がありません"))?
        };
        let clip: String = latex.chars().take(16_000).collect();
        let blob = run_embedder(script, &clip)?;
        {
            let conn = db.lock().map_err(|e| e.to_string())?;
            db::update_page_embedding(&conn, book_id, *page_num, &blob)
                .map_err(|e| e.to_string())?;
        }
        done += 1;
        let _ = app.emit(
            "embed-progress",
            serde_json::json!({
                "page": (i + 1) as i32,
                "total": total,
                "pageNum": page_num,
            }),
        );
    }
    // ── 概念単位の埋め込み（型プレフィックス + コンテキスト付き）──
    let concepts = {
        let conn = db.lock().map_err(|e| e.to_string())?;
        db::list_concepts_missing_embedding(&conn, book_id).map_err(|e| e.to_string())?
    };
    let concept_total = concepts.len();
    for concept in &concepts {
        let blob = run_embedder_concept(
            script,
            &concept.kind,
            concept.label.as_deref(),
            concept.name.as_deref(),
            &concept.latex,
            concept.context.as_deref(),
        );
        if let Ok(blob) = blob {
            let conn = db.lock().map_err(|e| e.to_string())?;
            db::update_concept_embedding(&conn, concept.id, &blob).ok();
        }
    }

    let _ = app.emit(
        "embed-done",
        serde_json::json!({ "count": done, "conceptCount": concept_total }),
    );
    Ok(done)
}
