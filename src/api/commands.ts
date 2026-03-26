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
