mod db;

use std::path::PathBuf;
use std::sync::Mutex;

use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

pub struct DbState(pub Mutex<rusqlite::Connection>);

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

#[tauri::command]
fn pick_pdf(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let path = app
        .dialog()
        .file()
        .add_filter("PDF", &["pdf"])
        .blocking_pick_file();
    Ok(path.map(|p| p.to_string()))
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
fn list_sessions(book_id: String, state: tauri::State<'_, DbState>) -> Result<Vec<serde_json::Value>, String> {
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
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = default_data_dir()?;
            let db_path = data_dir.join("math_teacher.db");
            let conn = db::open_and_migrate(&db_path).map_err(|e| e.to_string())?;
            app.manage(DbState(Mutex::new(conn)));
            app.manage(PathsState { db_path: db_path.clone() });
            if cfg!(debug_assertions) {
                eprintln!("[math-teacher] DB at {}", db_path.display());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_paths,
            pick_pdf,
            upsert_page_latex,
            create_session,
            list_sessions,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
