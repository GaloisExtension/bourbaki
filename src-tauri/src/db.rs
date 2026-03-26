//! SQLite schema per PLAN.md (FTS5; sqlite-vec deferred — embedding columns reserved).

use rusqlite::{params, Connection, Result as SqlResult};

use crate::openai::ConceptItem;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS books (
    id TEXT PRIMARY KEY,
    pdf_path TEXT NOT NULL,
    page_count INTEGER,
    created_at INTEGER DEFAULT (unixepoch())
);

CREATE TABLE IF NOT EXISTS pages (
    id INTEGER PRIMARY KEY,
    book_id TEXT NOT NULL,
    page_num INTEGER NOT NULL,
    latex TEXT NOT NULL DEFAULT '',
    embedding BLOB,
    created_at INTEGER DEFAULT (unixepoch()),
    UNIQUE(book_id, page_num)
);

CREATE VIRTUAL TABLE IF NOT EXISTS pages_fts USING fts5(
    latex,
    content='pages',
    content_rowid='id',
    tokenize='trigram'
);

CREATE TABLE IF NOT EXISTS concepts (
    id INTEGER PRIMARY KEY,
    book_id TEXT NOT NULL,
    page_num INTEGER NOT NULL,
    type TEXT NOT NULL,
    label TEXT,
    name TEXT,
    latex TEXT NOT NULL,
    embedding BLOB
);

CREATE TABLE IF NOT EXISTS concept_edges (
    id INTEGER PRIMARY KEY,
    from_id INTEGER NOT NULL,
    to_id INTEGER NOT NULL,
    edge_type TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    book_id TEXT NOT NULL,
    page_num INTEGER,
    selection_text TEXT,
    selection_latex TEXT,
    parent_id TEXT,
    resolved INTEGER DEFAULT 0,
    created_at INTEGER DEFAULT (unixepoch())
);

CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    is_compressed INTEGER DEFAULT 0,
    original_content TEXT,
    created_at INTEGER DEFAULT (unixepoch())
);

CREATE TABLE IF NOT EXISTS resolved_explanations (
    id INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL,
    summary TEXT NOT NULL,
    embedding BLOB,
    decay_weight REAL DEFAULT 1.0,
    created_at INTEGER DEFAULT (unixepoch())
);

CREATE VIRTUAL TABLE IF NOT EXISTS explanations_fts USING fts5(
    summary,
    content='resolved_explanations',
    content_rowid='id',
    tokenize='trigram'
);

CREATE INDEX IF NOT EXISTS idx_sessions_book ON sessions(book_id);
CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);
CREATE INDEX IF NOT EXISTS idx_concepts_book_page ON concepts(book_id, page_num);
"#;

pub fn open_and_migrate(path: &std::path::Path) -> SqlResult<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(SCHEMA)?;
    conn.execute("PRAGMA foreign_keys = ON;", [])?;
    Ok(conn)
}

pub fn upsert_book(
    conn: &Connection,
    id: &str,
    pdf_path: &str,
    page_count: i32,
) -> SqlResult<()> {
    conn.execute(
        "INSERT INTO books (id, pdf_path, page_count) VALUES (?1, ?2, ?3)
         ON CONFLICT(id) DO UPDATE SET
           pdf_path = excluded.pdf_path,
           page_count = excluded.page_count",
        params![id, pdf_path, page_count],
    )?;
    Ok(())
}

pub fn upsert_page_latex(
    conn: &Connection,
    book_id: &str,
    page_num: i32,
    latex: &str,
) -> SqlResult<i64> {
    conn.execute(
        "INSERT INTO pages (book_id, page_num, latex) VALUES (?1, ?2, ?3)
         ON CONFLICT(book_id, page_num) DO UPDATE SET latex = excluded.latex",
        params![book_id, page_num, latex],
    )?;
    let id: i64 = conn.query_row(
        "SELECT id FROM pages WHERE book_id = ?1 AND page_num = ?2",
        params![book_id, page_num],
        |row| row.get(0),
    )?;
    Ok(id)
}

pub fn replace_concepts_for_page(
    conn: &Connection,
    book_id: &str,
    page_num: i32,
    items: &[ConceptItem],
) -> SqlResult<()> {
    conn.execute(
        "DELETE FROM concepts WHERE book_id = ?1 AND page_num = ?2",
        params![book_id, page_num],
    )?;
    for it in items {
        let kind = normalize_concept_type(&it.kind);
        let latex_clip: String = it.latex.chars().take(8000).collect();
        conn.execute(
            "INSERT INTO concepts (book_id, page_num, type, label, name, latex)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                book_id,
                page_num,
                kind,
                it.label.as_deref(),
                it.name.as_deref(),
                latex_clip,
            ],
        )?;
    }
    Ok(())
}

fn normalize_concept_type(raw: &str) -> String {
    let r = raw.to_lowercase();
    match r.as_str() {
        "definition" | "theorem" | "lemma" | "example" | "proof" | "remark" | "other" => r,
        _ => "other".to_string(),
    }
}

/// ページ一覧（プレビュー用・先頭 200 文字）
pub fn list_pages_preview(conn: &Connection, book_id: &str) -> SqlResult<Vec<(i32, String)>> {
    let mut stmt = conn.prepare(
        "SELECT page_num, latex FROM pages WHERE book_id = ?1 ORDER BY page_num ASC",
    )?;
    let rows = stmt.query_map(params![book_id], |row| {
        let num: i32 = row.get(0)?;
        let latex: String = row.get(1)?;
        let preview = latex.chars().take(200).collect::<String>();
        Ok((num, preview))
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}
