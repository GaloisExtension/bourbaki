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
    context TEXT,
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
    migrate(&conn)?;
    conn.execute("PRAGMA foreign_keys = ON;", [])?;
    Ok(conn)
}

fn column_exists(conn: &Connection, table: &str, col: &str) -> SqlResult<bool> {
    let sql = format!("SELECT 1 FROM pragma_table_info('{table}') WHERE name = ?1");
    let mut stmt = conn.prepare(&sql)?;
    Ok(stmt.exists(params![col])?)
}

fn migrate(conn: &Connection) -> SqlResult<()> {
    let ver: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if ver < 1 {
        if !column_exists(conn, "resolved_explanations", "book_id")? {
            conn.execute(
                "ALTER TABLE resolved_explanations ADD COLUMN book_id TEXT",
                [],
            )?;
        }
        conn.execute(
            "UPDATE resolved_explanations SET book_id = (
                SELECT book_id FROM sessions WHERE sessions.id = resolved_explanations.session_id
            )
            WHERE book_id IS NULL",
            [],
        )?;
        conn.execute("PRAGMA user_version = 1", [])?;
    }
    if ver < 2 {
        if !column_exists(conn, "concepts", "context")? {
            conn.execute("ALTER TABLE concepts ADD COLUMN context TEXT", [])?;
        }
        conn.execute("PRAGMA user_version = 2", [])?;
    }
    Ok(())
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

pub fn list_books(conn: &Connection) -> SqlResult<Vec<serde_json::Value>> {
    let mut stmt = conn.prepare(
        "SELECT b.id, b.pdf_path, b.page_count, b.created_at,
                COUNT(DISTINCT p.page_num) AS indexed_pages,
                COUNT(DISTINCT CASE WHEN p.embedding IS NOT NULL AND length(p.embedding) >= 16 THEN p.page_num END) AS embedded_pages
         FROM books b
         LEFT JOIN pages p ON p.book_id = b.id
         GROUP BY b.id
         ORDER BY b.created_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(serde_json::json!({
            "id": row.get::<_, String>(0)?,
            "pdfPath": row.get::<_, String>(1)?,
            "pageCount": row.get::<_, Option<i32>>(2)?,
            "createdAt": row.get::<_, i64>(3)?,
            "indexedPages": row.get::<_, i64>(4)?,
            "embeddedPages": row.get::<_, i64>(5)?,
        }))
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn delete_book_cascade(conn: &Connection, book_id: &str) -> SqlResult<()> {
    // 関連するすべてのデータを削除（外部キーはOFFのため手動で削除）
    conn.execute("DELETE FROM concept_edges WHERE from_id IN (SELECT id FROM concepts WHERE book_id = ?1)", params![book_id])?;
    conn.execute("DELETE FROM concept_edges WHERE to_id IN (SELECT id FROM concepts WHERE book_id = ?1)", params![book_id])?;
    conn.execute("DELETE FROM concepts WHERE book_id = ?1", params![book_id])?;
    // messages → sessions の順で削除
    conn.execute(
        "DELETE FROM messages WHERE session_id IN (SELECT id FROM sessions WHERE book_id = ?1)",
        params![book_id],
    )?;
    conn.execute(
        "DELETE FROM resolved_explanations WHERE book_id = ?1",
        params![book_id],
    )?;
    conn.execute("DELETE FROM sessions WHERE book_id = ?1", params![book_id])?;
    conn.execute("DELETE FROM pages WHERE book_id = ?1", params![book_id])?;
    conn.execute("DELETE FROM books WHERE id = ?1", params![book_id])?;
    Ok(())
}

pub fn list_resolved_sessions(
    conn: &Connection,
    book_id: &str,
) -> SqlResult<Vec<serde_json::Value>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.page_num, s.selection_text, s.selection_latex, s.created_at,
                re.summary, re.created_at as resolved_at
         FROM sessions s
         LEFT JOIN resolved_explanations re ON re.session_id = s.id
         WHERE s.book_id = ?1 AND s.resolved = 1
         ORDER BY s.created_at DESC
         LIMIT 200",
    )?;
    let rows = stmt.query_map(params![book_id], |row| {
        Ok(serde_json::json!({
            "id": row.get::<_, String>(0)?,
            "pageNum": row.get::<_, Option<i32>>(1)?,
            "selectionText": row.get::<_, Option<String>>(2)?,
            "selectionLatex": row.get::<_, Option<String>>(3)?,
            "createdAt": row.get::<_, i64>(4)?,
            "summary": row.get::<_, Option<String>>(5)?,
            "resolvedAt": row.get::<_, Option<i64>>(6)?,
        }))
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
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
            "INSERT INTO concepts (book_id, page_num, type, label, name, latex, context)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                book_id,
                page_num,
                kind,
                it.label.as_deref(),
                it.name.as_deref(),
                latex_clip,
                it.context.as_deref(),
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

pub fn get_page_latex(
    conn: &Connection,
    book_id: &str,
    page_num: i32,
) -> SqlResult<Option<String>> {
    let mut stmt =
        conn.prepare("SELECT latex FROM pages WHERE book_id = ?1 AND page_num = ?2 LIMIT 1")?;
    let mut rows = stmt.query_map(params![book_id, page_num], |row| row.get::<_, String>(0))?;
    match rows.next() {
        Some(Ok(s)) => Ok(Some(s)),
        Some(Err(e)) => Err(e),
        None => Ok(None),
    }
}

pub fn list_pages_missing_embedding(conn: &Connection, book_id: &str) -> SqlResult<Vec<i32>> {
    let mut stmt = conn.prepare(
        "SELECT page_num FROM pages WHERE book_id = ?1 AND latex != ''
         AND (embedding IS NULL OR length(embedding) < 16)
         ORDER BY page_num ASC",
    )?;
    let rows = stmt.query_map(params![book_id], |row| row.get::<_, i32>(0))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn update_page_embedding(
    conn: &Connection,
    book_id: &str,
    page_num: i32,
    blob: &[u8],
) -> SqlResult<()> {
    conn.execute(
        "UPDATE pages SET embedding = ?1 WHERE book_id = ?2 AND page_num = ?3",
        params![blob, book_id, page_num],
    )?;
    Ok(())
}

/// 埋め込みが未生成の概念ID + (type, label, name, latex, context) を返す
pub struct ConceptEmbedRow {
    pub id: i64,
    pub kind: String,
    pub label: Option<String>,
    pub name: Option<String>,
    pub latex: String,
    pub context: Option<String>,
}

pub fn list_concepts_missing_embedding(
    conn: &Connection,
    book_id: &str,
) -> SqlResult<Vec<ConceptEmbedRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, type, label, name, latex, context FROM concepts
         WHERE book_id = ?1 AND latex != ''
         AND (embedding IS NULL OR length(embedding) < 16)
         ORDER BY id ASC",
    )?;
    let rows = stmt.query_map(params![book_id], |row| {
        Ok(ConceptEmbedRow {
            id: row.get(0)?,
            kind: row.get(1)?,
            label: row.get(2)?,
            name: row.get(3)?,
            latex: row.get(4)?,
            context: row.get(5)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn update_concept_embedding(conn: &Connection, concept_id: i64, blob: &[u8]) -> SqlResult<()> {
    conn.execute(
        "UPDATE concepts SET embedding = ?1 WHERE id = ?2",
        params![blob, concept_id],
    )?;
    Ok(())
}

/// 概念ベクトル検索用: 全概念の id + embedding を返す（RAG Agent用）
pub struct ConceptVecRow {
    pub id: i64,
    pub page_num: i32,
    pub kind: String,
    pub label: Option<String>,
    pub name: Option<String>,
    pub latex: String,
    pub embedding: Option<Vec<u8>>,
}

pub fn list_concept_rows_for_rag(
    conn: &Connection,
    book_id: &str,
) -> SqlResult<Vec<ConceptVecRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, page_num, type, label, name, latex, embedding FROM concepts
         WHERE book_id = ?1 ORDER BY id ASC",
    )?;
    let rows = stmt.query_map(params![book_id], |row| {
        Ok(ConceptVecRow {
            id: row.get(0)?,
            page_num: row.get(1)?,
            kind: row.get(2)?,
            label: row.get(3)?,
            name: row.get(4)?,
            latex: row.get(5)?,
            embedding: row.get(6)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// ページ一覧（プレビュー用・先頭 200 文字 + 埋め込み有無）
#[allow(dead_code)]
pub struct SessionRow {
    pub id: String,
    pub book_id: String,
    pub page_num: Option<i32>,
    pub selection_text: Option<String>,
    pub selection_latex: Option<String>,
    pub parent_id: Option<String>,
    pub resolved: bool,
}

pub fn get_session(conn: &Connection, id: &str) -> SqlResult<SessionRow> {
    conn.query_row(
        "SELECT id, book_id, page_num, selection_text, selection_latex, parent_id, resolved
         FROM sessions WHERE id = ?1",
        params![id],
        |row| {
            Ok(SessionRow {
                id: row.get(0)?,
                book_id: row.get(1)?,
                page_num: row.get(2)?,
                selection_text: row.get(3)?,
                selection_latex: row.get(4)?,
                parent_id: row.get(5)?,
                resolved: row.get::<_, i64>(6)? != 0,
            })
        },
    )
}

pub fn insert_message(
    conn: &Connection,
    session_id: &str,
    role: &str,
    content: &str,
) -> SqlResult<i64> {
    conn.execute(
        "INSERT INTO messages (session_id, role, content) VALUES (?1, ?2, ?3)",
        params![session_id, role, content],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_message_pairs(
    conn: &Connection,
    session_id: &str,
) -> SqlResult<Vec<(String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT role, content FROM messages WHERE session_id = ?1 ORDER BY id ASC",
    )?;
    let rows = stmt.query_map(params![session_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn list_messages_json(
    conn: &Connection,
    session_id: &str,
) -> SqlResult<Vec<serde_json::Value>> {
    let mut stmt = conn.prepare(
        "SELECT id, role, content, created_at FROM messages WHERE session_id = ?1 ORDER BY id ASC",
    )?;
    let rows = stmt.query_map(params![session_id], |row| {
        Ok(serde_json::json!({
            "id": row.get::<_, i64>(0)?,
            "role": row.get::<_, String>(1)?,
            "content": row.get::<_, String>(2)?,
            "createdAt": row.get::<_, i64>(3)?,
        }))
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn set_session_resolved(
    conn: &Connection,
    session_id: &str,
    resolved: bool,
) -> SqlResult<()> {
    let v = if resolved { 1 } else { 0 };
    conn.execute(
        "UPDATE sessions SET resolved = ?1 WHERE id = ?2",
        params![v, session_id],
    )?;
    Ok(())
}

pub fn branch_session(conn: &Connection, parent_id: &str) -> SqlResult<String> {
    let p = get_session(conn, parent_id)?;
    let id = ::uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO sessions (id, book_id, page_num, selection_text, selection_latex, parent_id, resolved)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
        params![
            id,
            p.book_id,
            p.page_num,
            p.selection_text,
            p.selection_latex,
            parent_id,
        ],
    )?;
    Ok(id)
}

pub fn delete_memory_for_session(conn: &Connection, session_id: &str) -> SqlResult<()> {
    conn.execute(
        "DELETE FROM resolved_explanations WHERE session_id = ?1",
        params![session_id],
    )?;
    Ok(())
}

pub fn insert_resolved_memory(
    conn: &Connection,
    session_id: &str,
    book_id: &str,
    summary: &str,
    embedding: &[u8],
) -> SqlResult<i64> {
    conn.execute(
        "INSERT INTO resolved_explanations (session_id, book_id, summary, embedding, decay_weight)
         VALUES (?1, ?2, ?3, ?4, 1.0)",
        params![session_id, book_id, summary, embedding],
    )?;
    Ok(conn.last_insert_rowid())
}

/// FTS5 OR クエリ用（空なら呼び出し側で弾く）
pub fn fts5_or_terms(user: &str) -> String {
    let parts: Vec<String> = user
        .split_whitespace()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .map(|t| {
            let safe = t.replace('\"', "");
            format!("\"{safe}\"")
        })
        .collect();
    if parts.is_empty() {
        String::new()
    } else {
        parts.join(" OR ")
    }
}

/// bm25 昇順（小さいほど関連が高い）で explanation rowid を返す
pub fn memory_fts_ranked_ids(
    conn: &Connection,
    book_id: &str,
    fts_match: &str,
    limit: i64,
) -> SqlResult<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT re.id FROM explanations_fts AS f
         INNER JOIN resolved_explanations AS re ON re.id = f.rowid
         WHERE f MATCH ?1 AND re.book_id = ?2
         ORDER BY bm25(f)
         LIMIT ?3",
    )?;
    let rows = stmt.query_map(params![fts_match, book_id, limit], |row| {
        row.get::<_, i64>(0)
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub struct MemoryRowLite {
    pub id: i64,
    pub session_id: String,
    pub summary: String,
    pub embedding: Option<Vec<u8>>,
    pub created_at: i64,
    pub decay_weight: f64,
}

pub fn list_memory_rows_book(conn: &Connection, book_id: &str) -> SqlResult<Vec<MemoryRowLite>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, summary, embedding, created_at, decay_weight
         FROM resolved_explanations WHERE book_id = ?1",
    )?;
    let rows = stmt.query_map(params![book_id], |row| {
        Ok(MemoryRowLite {
            id: row.get(0)?,
            session_id: row.get(1)?,
            summary: row.get(2)?,
            embedding: row.get(3)?,
            created_at: row.get(4)?,
            decay_weight: row.get(5)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// RAG用: pages の FTS5 検索（BM25順）→ rowid リスト
pub fn pages_fts_ranked_ids(
    conn: &Connection,
    book_id: &str,
    fts_match: &str,
    limit: i64,
) -> SqlResult<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT p.id FROM pages_fts AS f
         INNER JOIN pages AS p ON p.id = f.rowid
         WHERE f MATCH ?1 AND p.book_id = ?2
         ORDER BY bm25(f)
         LIMIT ?3",
    )?;
    let rows = stmt.query_map(params![fts_match, book_id, limit], |row| {
        row.get::<_, i64>(0)
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub struct PageRowLite {
    pub id: i64,
    pub page_num: i32,
    pub latex: String,
    pub embedding: Option<Vec<u8>>,
}

/// RAG用: book内の全ページ（embedding付き）を取得
pub fn list_page_rows_for_rag(conn: &Connection, book_id: &str) -> SqlResult<Vec<PageRowLite>> {
    let mut stmt = conn.prepare(
        "SELECT id, page_num, latex, embedding FROM pages
         WHERE book_id = ?1 AND latex != ''
         ORDER BY page_num ASC",
    )?;
    let rows = stmt.query_map(params![book_id], |row| {
        Ok(PageRowLite {
            id: row.get(0)?,
            page_num: row.get(1)?,
            latex: row.get(2)?,
            embedding: row.get(3)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// 指定ページ番号のLaTeXを取得（Context注入用）
pub fn get_pages_by_nums(
    conn: &Connection,
    book_id: &str,
    page_nums: &[i32],
) -> SqlResult<Vec<(i32, String)>> {
    if page_nums.is_empty() {
        return Ok(vec![]);
    }
    // ?1 = book_id, ?2..?N = page_nums
    let placeholders: Vec<String> = (1..=page_nums.len()).map(|i| format!("?{}", i + 1)).collect();
    let sql = format!(
        "SELECT page_num, latex FROM pages WHERE book_id = ?1 AND page_num IN ({}) ORDER BY page_num ASC",
        placeholders.join(",")
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    params_vec.push(Box::new(book_id.to_string()));
    for &n in page_nums {
        params_vec.push(Box::new(n));
    }
    let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(params_refs.as_slice(), |row| {
        Ok((row.get::<_, i32>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub struct ConceptNodeLite {
    pub id: i64,
    pub page_num: i32,
    pub kind: String,
    pub label: Option<String>,
    pub name: Option<String>,
    pub latex: String,
}

/// 指定ページに含まれる概念ノード一覧
pub fn concepts_by_page_nums(
    conn: &Connection,
    book_id: &str,
    page_nums: &[i32],
) -> SqlResult<Vec<ConceptNodeLite>> {
    if page_nums.is_empty() {
        return Ok(vec![]);
    }
    // ?1 = book_id, ?2..?N = page_nums
    let placeholders: Vec<String> = (1..=page_nums.len()).map(|i| format!("?{}", i + 1)).collect();
    let sql = format!(
        "SELECT id, page_num, type, label, name, latex FROM concepts
         WHERE book_id = ?1 AND page_num IN ({}) ORDER BY id ASC",
        placeholders.join(",")
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    params_vec.push(Box::new(book_id.to_string()));
    for &n in page_nums {
        params_vec.push(Box::new(n));
    }
    let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(params_refs.as_slice(), |row| {
        Ok(ConceptNodeLite {
            id: row.get(0)?,
            page_num: row.get(1)?,
            kind: row.get(2)?,
            label: row.get(3)?,
            name: row.get(4)?,
            latex: row.get(5)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// GraphRAG: 概念IDリストから依存グラフを辿って関連概念ページ番号を展開（BFS、depth=1〜2）
pub fn concept_deps_expand(
    conn: &Connection,
    concept_ids: &[i64],
    max_depth: u32,
) -> SqlResult<Vec<i64>> {
    if concept_ids.is_empty() || max_depth == 0 {
        return Ok(vec![]);
    }
    let mut visited: std::collections::HashSet<i64> = concept_ids.iter().cloned().collect();
    let mut frontier: Vec<i64> = concept_ids.to_vec();

    for _ in 0..max_depth {
        if frontier.is_empty() {
            break;
        }
        let placeholders: Vec<String> = (1..=frontier.len()).map(|i| format!("?{i}")).collect();
        let sql = format!(
            "SELECT DISTINCT to_id FROM concept_edges WHERE from_id IN ({})",
            placeholders.join(",")
        );
        let mut stmt = conn.prepare(&sql)?;
        let params_vec: Vec<Box<dyn rusqlite::ToSql>> =
            frontier.iter().map(|&id| Box::new(id) as Box<dyn rusqlite::ToSql>).collect();
        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(params_refs.as_slice(), |row| row.get::<_, i64>(0))?;
        let mut next = Vec::new();
        for r in rows {
            let id = r?;
            if visited.insert(id) {
                next.push(id);
            }
        }
        frontier = next;
    }
    Ok(visited.into_iter().collect())
}

/// 概念IDから概念ノードを取得
pub fn get_concepts_by_ids(conn: &Connection, ids: &[i64]) -> SqlResult<Vec<ConceptNodeLite>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("?{i}")).collect();
    let sql = format!(
        "SELECT id, page_num, type, label, name, latex FROM concepts WHERE id IN ({}) ORDER BY id ASC",
        placeholders.join(",")
    );
    let mut stmt = conn.prepare(&sql)?;
    let params_vec: Vec<Box<dyn rusqlite::ToSql>> =
        ids.iter().map(|&id| Box::new(id) as Box<dyn rusqlite::ToSql>).collect();
    let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(params_refs.as_slice(), |row| {
        Ok(ConceptNodeLite {
            id: row.get(0)?,
            page_num: row.get(1)?,
            kind: row.get(2)?,
            label: row.get(3)?,
            name: row.get(4)?,
            latex: row.get(5)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// コンテキスト圧縮: 古いメッセージをcompressedに更新
pub fn compress_messages(
    conn: &Connection,
    session_id: &str,
    summary: &str,
    ids_to_compress: &[i64],
) -> SqlResult<()> {
    if ids_to_compress.is_empty() {
        return Ok(());
    }
    // 圧縮サマリーをassistantメッセージとして挿入
    conn.execute(
        "INSERT INTO messages (session_id, role, content, is_compressed) VALUES (?1, 'assistant', ?2, 1)",
        params![session_id, format!("【会話圧縮サマリー】\n{summary}")],
    )?;
    // 対象メッセージを圧縮済みにマーク（?1..?N = ids）
    let placeholders: Vec<String> = (1..=ids_to_compress.len())
        .map(|i| format!("?{i}"))
        .collect();
    let sql = format!(
        "UPDATE messages SET is_compressed = 1, original_content = content, content = '（圧縮済み）'
         WHERE id IN ({})",
        placeholders.join(",")
    );
    let mut stmt = conn.prepare(&sql)?;
    let id_params: Vec<Box<dyn rusqlite::ToSql>> =
        ids_to_compress.iter().map(|&id| Box::new(id) as Box<dyn rusqlite::ToSql>).collect();
    let id_refs: Vec<&dyn rusqlite::ToSql> = id_params.iter().map(|p| p.as_ref()).collect();
    stmt.execute(id_refs.as_slice())?;
    Ok(())
}

/// メッセージをID付きで取得（圧縮チェック用）
pub fn list_message_pairs_with_ids(
    conn: &Connection,
    session_id: &str,
) -> SqlResult<Vec<(i64, String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT id, role, content FROM messages WHERE session_id = ?1 AND is_compressed = 0 ORDER BY id ASC",
    )?;
    let rows = stmt.query_map(params![session_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// メッセージのトークン数推定（英1文字≈1/4, 日本語1文字≈1〜2トークン概算）
pub fn estimate_message_tokens(content: &str) -> usize {
    let chars: Vec<char> = content.chars().collect();
    let mut total = 0usize;
    for c in &chars {
        if c.is_ascii() {
            total += 1;
        } else {
            total += 2; // CJK等
        }
    }
    (total / 4).max(1)
}

pub fn list_pages_preview(
    conn: &Connection,
    book_id: &str,
) -> SqlResult<Vec<(i32, String, bool)>> {
    let mut stmt = conn.prepare(
        "SELECT page_num, latex, embedding FROM pages WHERE book_id = ?1 ORDER BY page_num ASC",
    )?;
    let rows = stmt.query_map(params![book_id], |row| {
        let num: i32 = row.get(0)?;
        let latex: String = row.get(1)?;
        let emb: Option<Vec<u8>> = row.get(2)?;
        let preview = latex.chars().take(200).collect::<String>();
        let has_emb = emb.map(|b| b.len() >= 16).unwrap_or(false);
        Ok((num, preview, has_emb))
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}
