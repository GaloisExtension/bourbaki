mod openai;
mod db;
mod embed_sidecar;
mod ingest;
mod pdf_render;
mod selection_map;

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use ingest::IngestState;
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

pub struct DbState(pub Arc<Mutex<rusqlite::Connection>>);

pub struct PathsState {
    pub db_path: PathBuf,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
