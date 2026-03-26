import { invoke } from "@tauri-apps/api/core";

export async function pickPdf(): Promise<string | null> {
  return invoke<string | null>("pick_pdf");
}

export async function getPaths(): Promise<{ dbPath: string }> {
  return invoke("get_paths");
}

export async function upsertPageLatex(
  bookId: string,
  pageNum: number,
  latex: string,
): Promise<number> {
  return invoke<number>("upsert_page_latex", { bookId, pageNum, latex });
}

export async function createSession(payload: {
  bookId: string;
  pageNum?: number | null;
  selectionText?: string | null;
  selectionLatex?: string | null;
  parentId?: string | null;
}): Promise<string> {
  return invoke<string>("create_session", {
    bookId: payload.bookId,
    pageNum: payload.pageNum ?? null,
    selectionText: payload.selectionText ?? null,
    selectionLatex: payload.selectionLatex ?? null,
    parentId: payload.parentId ?? null,
  });
}

export async function startPdfIngest(payload: {
  bookId: string;
  pdfPath: string;
}): Promise<void> {
  await invoke("start_pdf_ingest", {
    bookId: payload.bookId,
    pdfPath: payload.pdfPath,
  });
}

export async function cancelPdfIngest(): Promise<void> {
  await invoke("cancel_pdf_ingest");
}

export async function listBookPages(bookId: string): Promise<
  { pageNum: number; preview: string; hasEmbedding: boolean }[]
> {
  return invoke("list_book_pages", { bookId });
}

export async function mapSelectionToLatex(payload: {
  bookId: string;
  pageNum: number;
  selectionText: string;
}): Promise<string | null> {
  return invoke("map_selection_to_latex", {
    bookId: payload.bookId,
    pageNum: payload.pageNum,
    selectionText: payload.selectionText,
  });
}

export async function sampleLinearAlgebraPdf(): Promise<string> {
  return invoke("sample_linear_algebra_pdf");
}

export async function embedBookPages(bookId: string): Promise<number> {
  return invoke<number>("embed_book_pages", { bookId });
}

export async function listSessions(bookId: string): Promise<
  {
    id: string;
    bookId: string;
    pageNum: number | null;
    selectionText: string | null;
    selectionLatex: string | null;
    parentId: string | null;
    resolved: number;
    createdAt: number;
  }[]
> {
  return invoke("list_sessions", { bookId });
}

export async function listSessionMessages(sessionId: string): Promise<
  { id: number; role: string; content: string; createdAt: number }[]
> {
  return invoke("list_session_messages", { sessionId });
}

export async function sendSessionMessage(payload: {
  sessionId: string;
  userText: string;
  thinkingEnabled: boolean;
}): Promise<string> {
  return invoke<string>("send_session_message", {
    sessionId: payload.sessionId,
    userText: payload.userText,
    thinkingEnabled: payload.thinkingEnabled,
  });
}

export async function setSessionResolved(
  sessionId: string,
  resolved: boolean,
): Promise<void> {
  await invoke("set_session_resolved_cmd", { sessionId, resolved });
}

export async function branchSession(parentId: string): Promise<string> {
  return invoke<string>("branch_session_cmd", { parentId });
}

export async function finalizeSessionMemory(sessionId: string): Promise<{
  summary: string;
  dim: number;
}> {
  return invoke("finalize_session_memory", { sessionId });
}

export async function listBooks(): Promise<
  {
    id: string;
    pdfPath: string;
    pageCount: number | null;
    createdAt: number;
    indexedPages: number;
    embeddedPages: number;
  }[]
> {
  return invoke("list_books");
}

export async function deleteBook(bookId: string): Promise<void> {
  await invoke("delete_book", { bookId });
}

export async function listResolvedSessions(bookId: string): Promise<
  {
    id: string;
    pageNum: number | null;
    selectionText: string | null;
    selectionLatex: string | null;
    createdAt: number;
    summary: string | null;
    resolvedAt: number | null;
  }[]
> {
  return invoke("list_resolved_sessions", { bookId });
}

export async function prefetchPages(payload: {
  bookId: string;
  centerPage: number;
}): Promise<void> {
  await invoke("prefetch_pages", {
    bookId: payload.bookId,
    centerPage: payload.centerPage,
  });
}

export async function memorySearch(payload: {
  bookId: string;
  query: string;
  limit?: number;
}): Promise<
  { id: number; sessionId: string; summary: string; score: number }[]
> {
  return invoke("memory_search", {
    bookId: payload.bookId,
    query: payload.query,
    limit: payload.limit ?? null,
  });
}

// ── ChatGPT セッション管理 ──────────────────

export async function openChatgptLogin(): Promise<void> {
  await invoke("open_chatgpt_login");
}

export async function saveChatgptSession(accessToken: string): Promise<void> {
  await invoke("save_chatgpt_session", { accessToken });
}

export async function logoutChatgpt(): Promise<void> {
  await invoke("logout_chatgpt");
}

export async function getSettings(): Promise<{
  chatgptLoggedIn: boolean;
  hasVisionKey: boolean;
}> {
  return invoke("get_settings");
}

export async function saveVisionApiKey(key: string): Promise<void> {
  await invoke("save_vision_api_key", { key });
}
