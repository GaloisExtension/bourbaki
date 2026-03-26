import { useCallback, useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import "./App.css";
import { PdfViewer } from "./components/PdfViewer/PdfViewer";
import { FlowCanvas } from "./components/Canvas/FlowCanvas";
import { useAppStore } from "./store/appStore";
import {
  cancelPdfIngest,
  createSession,
  getPaths,
  listBookPages,
  listSessions,
  pickPdf,
  startPdfIngest,
} from "./api/commands";

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
  const pdfPath = useAppStore((s) => s.pdfPath);

  const [ingestBusy, setIngestBusy] = useState(false);
  const [ingestLine, setIngestLine] = useState<string | null>(null);
  const [ingestError, setIngestError] = useState<string | null>(null);
  const [pagesIndexed, setPagesIndexed] = useState(0);

  useEffect(() => {
    getPaths()
      .then((p) => setDbPathHint(p.dbPath))
      .catch(console.error);
  }, [setDbPathHint]);

  const refreshPageIndex = useCallback(() => {
    if (!bookId) return;
    listBookPages(bookId)
      .then((rows) => setPagesIndexed(rows.length))
      .catch(console.error);
  }, [bookId]);

  useEffect(() => {
    refreshPageIndex();
  }, [refreshPageIndex]);

  useEffect(() => {
    let disposed = false;
    const unlisteners: UnlistenFn[] = [];

    void (async () => {
      const reg = async (name: string, cb: Parameters<typeof listen>[1]) => {
        const u = await listen(name, cb);
        if (disposed) u();
        else unlisteners.push(u);
      };
      await reg("ingest-progress", (e) => {
        const p = e.payload as {
          phase?: string;
          page?: number;
          total?: number;
          message?: string;
        };
        const total = p.total ?? "?";
        const page = p.page ?? "?";
        const msg = p.message ?? p.phase ?? "";
        setIngestLine(`${p.phase}: ${page}/${total} ${msg}`.trim());
      });
      await reg("ingest-error", (e) => {
        setIngestError(String(e.payload));
        setIngestBusy(false);
      });
      await reg("ingest-done", (e) => {
        const ok = (e.payload as { ok?: boolean })?.ok ?? false;
        setIngestBusy(false);
        setIngestLine(ok ? "取り込みが完了しました" : "取り込みが中断されました");
        refreshPageIndex();
      });
    })().catch(console.error);

    return () => {
      disposed = true;
      unlisteners.forEach((u) => u());
    };
  }, [refreshPageIndex]);

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
    setIngestError(null);
    setIngestLine(null);
  }, [setPdf]);

  const runIngest = useCallback(async () => {
    if (!pdfPath || ingestBusy) return;
    setIngestError(null);
    setIngestBusy(true);
    setIngestLine("キュー開始…");
    try {
      await startPdfIngest({ bookId, pdfPath });
    } catch (err) {
      setIngestBusy(false);
      setIngestError(String(err));
    }
  }, [bookId, pdfPath, ingestBusy]);

  const stopIngest = useCallback(async () => {
    await cancelPdfIngest();
  }, []);

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
              Phase 2: Poppler + Vision LaTeX + 概念抽出（DB）
            </div>
          </div>
        </div>
        <div className="app-toolbar">
          <button type="button" className="btn btn--ghost" onClick={openPdf}>
            PDFを開く
          </button>
          <button
            type="button"
            className="btn btn--accent"
            disabled={!pdfPath || ingestBusy}
            onClick={runIngest}
            title="OPENAI_API_KEY と poppler (pdftoppm) が必要です"
          >
            LaTeX 取り込み
          </button>
          <button
            type="button"
            className="btn btn--muted"
            disabled={!ingestBusy}
            onClick={stopIngest}
          >
            取り込み停止
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
          <span className="app-meta__txt">
            索引ページ {pagesIndexed} / book{" "}
            <code className="app-meta__code">{bookId.slice(0, 8)}…</code>
          </span>
          {ingestLine ? (
            <span className="app-meta__ingest">{ingestLine}</span>
          ) : null}
          {ingestError ? (
            <span className="app-meta__err" title={ingestError}>
              {ingestError.slice(0, 80)}
              {ingestError.length > 80 ? "…" : ""}
            </span>
          ) : null}
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
