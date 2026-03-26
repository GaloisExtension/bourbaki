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

pub fn run_embedder(script: &Path, text: &str) -> Result<Vec<u8>, String> {
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
    let payload = serde_json::json!({ "text": text }).to_string();
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
    let _ = app.emit("embed-done", serde_json::json!({ "count": done }));
    Ok(done)
}
