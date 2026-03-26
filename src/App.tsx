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
  embedBookPages,
  getPaths,
  listBookPages,
  listSessions,
  mapSelectionToLatex,
  memorySearch,
  pickPdf,
  prefetchPages,
  sampleLinearAlgebraPdf,
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
  const setSessionRows = useAppStore((s) => s.setSessionRows);
  const chatVersion = useAppStore((s) => s.chatVersion);
  const setSelectionLatexMapped = useAppStore((s) => s.setSelectionLatexMapped);
  const selectionTextForMap = useAppStore((s) => s.selectionText);
  const selectionPageForMap = useAppStore((s) => s.selectionPage);
  const dbPathHint = useAppStore((s) => s.dbPathHint);
  const pdfPath = useAppStore((s) => s.pdfPath);
  const updateAgentStatus = useAppStore((s) => s.updateAgentStatus);
  const addCompressedSession = useAppStore((s) => s.addCompressedSession);

  const [ingestBusy, setIngestBusy] = useState(false);
  const [ingestLine, setIngestLine] = useState<string | null>(null);
  const [ingestError, setIngestError] = useState<string | null>(null);
  const [pagesIndexed, setPagesIndexed] = useState(0);
  const [pagesEmbedded, setPagesEmbedded] = useState(0);
  const [embedBusy, setEmbedBusy] = useState(false);
  const [embedLine, setEmbedLine] = useState<string | null>(null);
  const [memQ, setMemQ] = useState("");
  const [memHits, setMemHits] = useState<
    { id: number; sessionId: string; summary: string; score: number }[]
  >([]);
  const [memBusy, setMemBusy] = useState(false);
  const [memErr, setMemErr] = useState<string | null>(null);

  useEffect(() => {
    getPaths()
      .then((p) => setDbPathHint(p.dbPath))
      .catch(console.error);
  }, [setDbPathHint]);

  const refreshPageIndex = useCallback(() => {
    if (!bookId) return;
    listBookPages(bookId)
      .then((rows) => {
        setPagesIndexed(rows.length);
        setPagesEmbedded(rows.filter((r) => r.hasEmbedding).length);
      })
      .catch(console.error);
  }, [bookId]);

  useEffect(() => {
    if (!selectionTextForMap.trim() || selectionPageForMap == null) {
      setSelectionLatexMapped(null);
      return;
    }
    const t = window.setTimeout(() => {
      mapSelectionToLatex({
        bookId,
        pageNum: selectionPageForMap,
        selectionText: selectionTextForMap,
      })
        .then((s) => setSelectionLatexMapped(s))
        .catch(() => setSelectionLatexMapped(null));
      // Prefetch Agent: 選択ページ周辺を先読み（バックグラウンド）
      prefetchPages({ bookId, centerPage: selectionPageForMap }).catch(() => {});
    }, 380);
    return () => window.clearTimeout(t);
  }, [
    selectionTextForMap,
    selectionPageForMap,
    bookId,
    setSelectionLatexMapped,
  ]);

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
      await reg("embed-progress", (e) => {
        const p = e.payload as {
          page?: number;
          total?: number;
          pageNum?: number;
        };
        setEmbedLine(
          `埋め込み ${p.page ?? "?"}/${p.total ?? "?"} (PDF p.${p.pageNum ?? "?"})`,
        );
      });
      await reg("embed-done", () => {
        setEmbedBusy(false);
        refreshPageIndex();
      });
      await reg("agent-status", (e) => {
        const p = e.payload as {
          agent: string;
          status: "running" | "done" | "error";
          detail: string;
        };
        updateAgentStatus(p);
      });
      await reg("compression-done", (e) => {
        const p = e.payload as { sessionId?: string };
        if (p.sessionId) addCompressedSession(p.sessionId);
      });
    })().catch(console.error);

    return () => {
      disposed = true;
      unlisteners.forEach((u) => u());
    };
  }, [refreshPageIndex, updateAgentStatus, addCompressedSession]);

  useEffect(() => {
    if (!bookId) return;
    listSessions(bookId)
      .then((rows) =>
        setSessionRows(
          rows.map((r) => ({
            id: r.id,
            bookId: r.bookId,
            pageNum: r.pageNum,
            selectionText: r.selectionText,
            selectionLatex: r.selectionLatex,
            parentId: r.parentId,
            resolved: r.resolved !== 0,
            createdAt: r.createdAt,
          })),
        ),
      )
      .catch(console.error);
  }, [bookId, chatVersion, setSessionRows]);

  const newThread = useCallback(async () => {
    const st = useAppStore.getState();
    try {
      await createSession({
        bookId: st.bookId,
        pageNum: st.selectionPage,
        selectionText: st.selectionText || null,
        selectionLatex: st.selectionLatexMapped || null,
        parentId: null,
      });
      st.bumpChatVersion();
    } catch (e) {
      console.error(e);
    }
  }, []);

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

  const openSamplePdf = useCallback(async () => {
    try {
      const path = await sampleLinearAlgebraPdf();
      const asset = convertFileSrc(path);
      setPdf(path, asset);
      setIngestError(null);
      setIngestLine(null);
    } catch (e) {
      setIngestError(String(e));
    }
  }, [setPdf]);

  const runEmbed = useCallback(async () => {
    if (!bookId || embedBusy || pagesIndexed === 0) return;
    setEmbedBusy(true);
    setEmbedLine("埋め込みキュー…");
    setIngestError(null);
    try {
      await embedBookPages(bookId);
    } catch (e) {
      setIngestError(String(e));
      setEmbedLine(null);
    } finally {
      setEmbedBusy(false);
      refreshPageIndex();
    }
  }, [bookId, embedBusy, pagesIndexed, refreshPageIndex]);

  const runMemSearch = useCallback(async () => {
    const q = memQ.trim();
    if (!q || !bookId) return;
    setMemBusy(true);
    setMemErr(null);
    try {
      const hits = await memorySearch({ bookId, query: q, limit: 8 });
      setMemHits(hits);
    } catch (e) {
      setMemErr(String(e));
      setMemHits([]);
    } finally {
      setMemBusy(false);
    }
  }, [bookId, memQ]);

  return (
    <div className="app-shell">
      <header className="app-header">
        <div className="app-brand">
          <span className="app-brand__mark">∫</span>
          <div>
            <div className="app-brand__title">Math Teacher</div>
            <div className="app-brand__sub">
              Phase 5: RAG + Context + Adversarial + 透明性UI
            </div>
          </div>
        </div>
        <div className="app-toolbar">
          <button type="button" className="btn btn--ghost" onClick={openPdf}>
            PDFを開く
          </button>
          <button
            type="button"
            className="btn btn--ghost"
            onClick={openSamplePdf}
            title="リポジトリ直下の text_linear_algebra.pdf"
          >
            サンプルPDF
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
          <button
            type="button"
            className="btn btn--ghost"
            disabled={pagesIndexed === 0 || embedBusy}
            onClick={runEmbed}
            title="sidecar: pip install -r sidecar/requirements.txt"
          >
            ベクトル化
          </button>
          <button
            type="button"
            className="btn btn--ghost"
            onClick={() => void newThread()}
            title="現在の選択をコンテキストにした空スレッドを追加（キャンバスにノードが増えます）"
          >
            新しいスレッド
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
            索引 {pagesIndexed} ・ 埋め込み {pagesEmbedded} /{" "}
            <code className="app-meta__code">{bookId.slice(0, 8)}…</code>
          </span>
          {embedLine ? (
            <span className="app-meta__ingest">{embedLine}</span>
          ) : null}
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
      <section className="memory-bar" aria-label="解決済み記憶の検索">
        <span className="memory-bar__label">記憶</span>
        <input
          type="search"
          className="memory-bar__input"
          placeholder="キーワード（FTS+ベクトル）"
          value={memQ}
          onChange={(e) => setMemQ(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void runMemSearch();
          }}
        />
        <button
          type="button"
          className="btn btn--ghost btn--sm"
          disabled={!bookId || memBusy || !memQ.trim()}
          title="解決済みセッションの要約を検索（python3 + 埋め込みモデル）"
          onClick={() => void runMemSearch()}
        >
          {memBusy ? "検索中…" : "検索"}
        </button>
        {memErr ? (
          <span className="memory-bar__err" title={memErr}>
            {memErr.slice(0, 72)}
            {memErr.length > 72 ? "…" : ""}
          </span>
        ) : null}
        <div className="memory-bar__hits">
          {memHits.map((h) => (
            <details key={h.id} className="memory-hit">
              <summary className="memory-hit__sum">
                <span className="memory-hit__score">
                  {typeof h.score === "number" ? h.score.toFixed(4) : h.score}
                </span>
                <code>{h.sessionId.slice(0, 8)}…</code>
              </summary>
              <pre className="memory-hit__body">{h.summary}</pre>
            </details>
          ))}
        </div>
      </section>
      <main className="app-main">
        <section className="pane pane--pdf">
          <PdfViewer
            fileUrl={pdfAssetUrl}
            onSelectionChange={(t, p) => setSelection(t, p)}
          />
        </section>
        <section className="pane pane--flow">
          <FlowCanvas />
        </section>
      </main>
    </div>
  );
}

export default App;
