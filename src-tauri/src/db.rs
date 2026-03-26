//! SQLite schema per PLAN.md (FTS5; sqlite-vec deferred — embedding columns reserved).

use rusqlite::{params, Connection, Result as SqlResult};

const SCHEMA: &str = r#"
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
"#;

pub fn open_and_migrate(path: &std::path::Path) -> SqlResult<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(SCHEMA)?;
    conn.execute("PRAGMA foreign_keys = ON;", [])?;
    Ok(conn)
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
