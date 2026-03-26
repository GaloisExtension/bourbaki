import { create } from "zustand";

export type SessionRef = {
  id: string;
  label: string;
};

export type AppStore = {
  bookId: string;
  pdfPath: string | null;
  /** Tauri convertFileSrc URL for PDF.js */
  pdfAssetUrl: string | null;
  selectionText: string;
  selectionPage: number | null;
  sessions: SessionRef[];
  thinkingEnabled: boolean;
  dbPathHint: string | null;
  setBookId: (id: string) => void;
  setPdf: (path: string | null, assetUrl: string | null) => void;
  setSelection: (text: string, page: number | null) => void;
  addSession: (s: SessionRef) => void;
  setSessions: (sessions: SessionRef[]) => void;
  setThinkingEnabled: (v: boolean) => void;
  setDbPathHint: (p: string | null) => void;
};

export const useAppStore = create<AppStore>((set) => ({
  bookId: "default",
  pdfPath: null,
  pdfAssetUrl: null,
  selectionText: "",
  selectionPage: null,
  sessions: [],
  thinkingEnabled: true,
  dbPathHint: null,
  setBookId: (bookId) => set({ bookId }),
  setPdf: (pdfPath, pdfAssetUrl) =>
    set({ pdfPath, pdfAssetUrl, selectionText: "", selectionPage: null }),
  setSelection: (selectionText, selectionPage) => set({ selectionText, selectionPage }),
  addSession: (s) => set((st) => ({ sessions: [s, ...st.sessions] })),
  setSessions: (sessions) => set({ sessions }),
  setThinkingEnabled: (thinkingEnabled) => set({ thinkingEnabled }),
  setDbPathHint: (dbPathHint) => set({ dbPathHint }),
}));
