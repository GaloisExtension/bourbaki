//! PDF 取り込みパイプライン（画像化 → Vision LaTeX → DB、任意で概念抽出）。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rusqlite::Connection;
use tauri::{AppHandle, Emitter};

use crate::db;
use crate::openai;
use crate::pdf_render;

/// 取り込みキャンセル・多重起動防止。
#[derive(Clone)]
pub struct IngestState {
    pub cancel: Arc<AtomicBool>,
    pub busy: Arc<AtomicBool>,
}

impl IngestState {
    pub fn new() -> Self {
        Self {
            cancel: Arc::new(AtomicBool::new(false)),
            busy: Arc::new(AtomicBool::new(false)),
        }
    }
}

struct ClearBusy(Arc<AtomicBool>);

impl Drop for ClearBusy {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn emit_ingest_done(app: &AppHandle, ok: bool) {
    let _ = app.emit("ingest-done", serde_json::json!({ "ok": ok }));
}

pub async fn run_ingestion(
    app: AppHandle,
    db: Arc<std::sync::Mutex<Connection>>,
    ingest: IngestState,
    book_id: String,
    pdf_path: PathBuf,
) {
    let _busy_guard = ClearBusy(ingest.busy.clone());

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .connect_timeout(Duration::from_secs(30))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            let _ = app.emit("ingest-error", format!("HTTP クライアント: {e}"));
            emit_ingest_done(&app, false);
            return;
        }
    };

    let api_key = match std::env::var("OPENAI_API_KEY") {
        Ok(k) if !k.is_empty() => k,
        _ => {
            let _ = app.emit(
                "ingest-error",
                "OPENAI_API_KEY が空です。.env またはシェルに設定してください。",
            );
            emit_ingest_done(&app, false);
            return;
        }
    };

    let vision_model = std::env::var("MATH_TEACHER_VISION_MODEL")
        .unwrap_or_else(|_| "gpt-4o".to_string());
    let mini_model = std::env::var("MATH_TEACHER_CONCEPT_MODEL")
        .unwrap_or_else(|_| "gpt-4o-mini".to_string());
    let skip_concepts = std::env::var("MATH_TEACHER_SKIP_CONCEPTS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    if !pdf_render::pdftoppm_available() {
        let _ = app.emit(
            "ingest-error",
            "pdftoppm がありません。macOS: brew install poppler",
        );
        emit_ingest_done(&app, false);
        return;
    }

    let total = match pdf_render::page_count(&pdf_path) {
        Ok(n) => n as i32,
        Err(e) => {
            let _ = app.emit("ingest-error", e);
            emit_ingest_done(&app, false);
            return;
        }
    };

    {
        let conn = match db.lock() {
            Ok(c) => c,
            Err(e) => {
                let _ = app.emit("ingest-error", e.to_string());
                emit_ingest_done(&app, false);
                return;
            }
        };
        if let Err(e) = db::upsert_book(
            &conn,
            &book_id,
            &pdf_path.to_string_lossy(),
            total,
        ) {
            let _ = app.emit("ingest-error", e.to_string());
            emit_ingest_done(&app, false);
            return;
        }
    }

    let mut stopped = false;

    for p in 1..=total {
        if ingest.cancel.load(Ordering::SeqCst) {
            let _ = app.emit(
                "ingest-progress",
                serde_json::json!({
                    "phase": "cancelled",
                    "page": p.saturating_sub(1),
                    "total": total,
                }),
            );
            stopped = true;
            break;
        }

        let _ = app.emit(
            "ingest-progress",
            serde_json::json!({
                "phase": "render",
                "page": p,
                "total": total,
                "message": format!("{p}/{total} 画像化"),
            }),
        );

        let td = match tempfile::tempdir() {
            Ok(d) => d,
            Err(e) => {
                let _ = app.emit("ingest-error", e.to_string());
                stopped = true;
                break;
            }
        };
        let prefix = td.path().join("page");
        let png_path = match pdf_render::render_page_png(&pdf_path, p as u32, &prefix) {
            Ok(path) => path,
            Err(e) => {
                let _ = app.emit("ingest-error", e);
                stopped = true;
                break;
            }
        };
        let bytes = match std::fs::read(&png_path) {
            Ok(b) => b,
            Err(e) => {
                let _ = app.emit("ingest-error", e.to_string());
                stopped = true;
                break;
            }
        };

        let _ = app.emit(
            "ingest-progress",
            serde_json::json!({
                "phase": "vision",
                "page": p,
                "total": total,
                "message": "Vision → LaTeX",
            }),
        );

        let latex =
            match openai::transcribe_page_to_latex(&client, &api_key, &vision_model, &bytes).await
            {
                Ok(l) => l,
                Err(e) => {
                    let _ = app.emit("ingest-error", e);
                    stopped = true;
                    break;
                }
            };

        {
            let conn = match db.lock() {
                Ok(c) => c,
                Err(e) => {
                    let _ = app.emit("ingest-error", e.to_string());
                    stopped = true;
                    break;
                }
            };
            if let Err(e) = db::upsert_page_latex(&conn, &book_id, p, &latex) {
                let _ = app.emit("ingest-error", e.to_string());
                stopped = true;
                break;
            }
        }

        if !skip_concepts {
            let _ = app.emit(
                "ingest-progress",
                serde_json::json!({
                    "phase": "concepts",
                    "page": p,
                    "total": total,
                    "message": "概念抽出 (mini)",
                }),
            );
            match openai::extract_concepts_json(&client, &api_key, &mini_model, &latex).await {
                Ok(items) => {
                    if let Ok(conn) = db.lock() {
                        if let Err(e) =
                            db::replace_concepts_for_page(&conn, &book_id, p, &items)
                        {
                            let _ = app.emit(
                                "ingest-progress",
                                serde_json::json!({
                                    "phase": "concept_warn",
                                    "page": p,
                                    "message": e.to_string()
                                }),
                            );
                        }
                    }
                }
                Err(e) => {
                    let _ = app.emit(
                        "ingest-progress",
                        serde_json::json!({
                            "phase": "concept_warn",
                            "page": p,
                            "message": e
                        }),
                    );
                }
            }
        }

        let _ = app.emit(
            "ingest-progress",
            serde_json::json!({
                "phase": "page_done",
                "page": p,
                "total": total,
            }),
        );
    }

    ingest.cancel.store(false, Ordering::SeqCst);
    emit_ingest_done(&app, !stopped);
}
