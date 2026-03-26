import { useCallback, useEffect } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import "./App.css";
import { PdfViewer } from "./components/PdfViewer/PdfViewer";
import { FlowCanvas } from "./components/Canvas/FlowCanvas";
import { useAppStore } from "./store/appStore";
import { createSession, getPaths, listSessions, pickPdf } from "./api/commands";

function App() {
  const pdfAssetUrl = useAppStore((s) => s.pdfAssetUrl);
  const bookId = useAppStore((s) => s.bookId);
  const thinkingEnabled = useAppStore((s) => s.thinkingEnabled);
  const setPdf = useAppStore((s) => s.setPdf);
  const setSelection = useAppStore((s) => s.setSelection);
  const setThinkingEnabled = useAppStore((s) => s.setThinkingEnabled);
  const setDbPathHint = useAppStore((s) => s.setDbPathHint);
  const setSessions = useAppStore((s) => s.setSessions);
  const addSession = useAppStore((s) => s.addSession);
  const dbPathHint = useAppStore((s) => s.dbPathHint);

  useEffect(() => {
    getPaths()
      .then((p) => setDbPathHint(p.dbPath))
      .catch(console.error);
  }, [setDbPathHint]);

  useEffect(() => {
    if (!bookId) return;
    listSessions(bookId)
      .then((rows) =>
        setSessions(
          rows.map((r) => ({
            id: r.id,
            label:
              r.selectionText?.slice(0, 28) ||
              `session ${r.id.slice(0, 8)}…`,
          })),
        ),
      )
      .catch(console.error);
  }, [bookId, setSessions]);

  const openPdf = useCallback(async () => {
    const path = await pickPdf();
    if (!path) return;
    const asset = convertFileSrc(path);
    setPdf(path, asset);
  }, [setPdf]);

  const onChatSubmit = useCallback(
    async (text: string) => {
      if (!text.trim()) return;
      const selectionText = useAppStore.getState().selectionText;
      const selectionPage = useAppStore.getState().selectionPage;
      try {
        const id = await createSession({
          bookId,
          pageNum: selectionPage,
          selectionText: selectionText || null,
          selectionLatex: null,
          parentId: null,
        });
        addSession({
          id,
          label: text.slice(0, 40),
        });
      } catch (e) {
        console.error(e);
      }
    },
    [bookId, addSession],
  );

  return (
    <div className="app-shell">
      <header className="app-header">
        <div className="app-brand">
          <span className="app-brand__mark">∫</span>
          <div>
            <div className="app-brand__title">Math Teacher</div>
            <div className="app-brand__sub">
              PDF + ブランチ会話（Phase 1〜3 コア）
            </div>
          </div>
        </div>
        <div className="app-toolbar">
          <button type="button" className="btn btn--ghost" onClick={openPdf}>
            PDFを開く
          </button>
          <label className="toggle">
            <input
              type="checkbox"
              checked={thinkingEnabled}
              onChange={(e) => setThinkingEnabled(e.target.checked)}
            />
            <span>Thinking（🧠 / ⚡）</span>
          </label>
        </div>
        <div className="app-meta" title={dbPathHint ?? ""}>
          <span className="token-bar" aria-hidden>
            <span className="token-bar__fill token-bar__fill--mock" />
          </span>
          <span className="app-meta__txt">DB: 接続済み</span>
        </div>
      </header>
      <main className="app-main">
        <section className="pane pane--pdf">
          <PdfViewer
            fileUrl={pdfAssetUrl}
            onSelectionChange={(t, p) => setSelection(t, p)}
          />
        </section>
        <section className="pane pane--flow">
          <FlowCanvas onChatSubmit={onChatSubmit} />
        </section>
      </main>
    </div>
  );
}

export default App;
