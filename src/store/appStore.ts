import { create } from "zustand";

export type SessionDetail = {
  id: string;
  bookId: string;
  pageNum: number | null;
  selectionText: string | null;
  selectionLatex: string | null;
  parentId: string | null;
  resolved: boolean;
  createdAt: number;
};

export type ChatMessageRow = {
  id: number;
  role: string;
  content: string;
  createdAt: number;
};

export type AgentStatus = {
  agent: string;
  status: "running" | "done" | "error";
  detail: string;
  updatedAt: number;
};

export type AppStore = {
  bookId: string;
  pdfPath: string | null;
  /** Tauri convertFileSrc URL for PDF.js */
  pdfAssetUrl: string | null;
  selectionText: string;
  selectionPage: number | null;
  /** DB ページ LaTeX から推定した選択スニペット */
  selectionLatexMapped: string | null;
  sessionRows: SessionDetail[];
  /** listSessions / メッセージ送信後に進めると Flow が再取得 */
  chatVersion: number;
  thinkingEnabled: boolean;
  dbPathHint: string | null;
  /** 現在のエージェント実行ステータス（agent-status イベントで更新） */
  agentStatuses: AgentStatus[];
  /** コンテキスト圧縮が走ったセッションIDリスト */
  compressedSessions: string[];
  setBookId: (id: string) => void;
  setPdf: (path: string | null, assetUrl: string | null) => void;
  setSelection: (text: string, page: number | null) => void;
  setSelectionLatexMapped: (latex: string | null) => void;
  setSessionRows: (rows: SessionDetail[]) => void;
  bumpChatVersion: () => void;
  setThinkingEnabled: (v: boolean) => void;
  setDbPathHint: (p: string | null) => void;
  updateAgentStatus: (status: Omit<AgentStatus, "updatedAt">) => void;
  clearAgentStatuses: () => void;
  addCompressedSession: (sessionId: string) => void;
};

export const useAppStore = create<AppStore>((set) => ({
  bookId: "default",
  pdfPath: null,
  pdfAssetUrl: null,
  selectionText: "",
  selectionPage: null,
  selectionLatexMapped: null,
  sessionRows: [],
  chatVersion: 0,
  thinkingEnabled: true,
  dbPathHint: null,
  agentStatuses: [],
  compressedSessions: [],
  setBookId: (bookId) => set({ bookId }),
  setPdf: (pdfPath, pdfAssetUrl) =>
    set((st) => ({
      pdfPath,
      pdfAssetUrl,
      selectionText: "",
      selectionPage: null,
      selectionLatexMapped: null,
      bookId: pdfPath ? crypto.randomUUID() : st.bookId,
      sessionRows: [],
      chatVersion: st.chatVersion + 1,
    })),
  setSelection: (selectionText, selectionPage) =>
    set({ selectionText, selectionPage, selectionLatexMapped: null }),
  setSelectionLatexMapped: (selectionLatexMapped) => set({ selectionLatexMapped }),
  setSessionRows: (sessionRows) => set({ sessionRows }),
  bumpChatVersion: () => set((st) => ({ chatVersion: st.chatVersion + 1 })),
  setThinkingEnabled: (thinkingEnabled) => set({ thinkingEnabled }),
  setDbPathHint: (dbPathHint) => set({ dbPathHint }),
  updateAgentStatus: (status) =>
    set((st) => {
      const existing = st.agentStatuses.filter((s) => s.agent !== status.agent);
      return {
        agentStatuses: [
          ...existing,
          { ...status, updatedAt: Date.now() },
        ],
      };
    }),
  clearAgentStatuses: () => set({ agentStatuses: [] }),
  addCompressedSession: (sessionId) =>
    set((st) => ({
      compressedSessions: st.compressedSessions.includes(sessionId)
        ? st.compressedSessions
        : [...st.compressedSessions, sessionId],
    })),
}));
