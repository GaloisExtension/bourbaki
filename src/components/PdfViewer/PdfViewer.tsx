import { useCallback, useEffect, useRef, useState } from "react";
import {
  getDocument,
  TextLayer,
  type PDFDocumentProxy,
  type PDFPageProxy,
} from "pdfjs-dist";
import "./pdf-viewer.css";
import { getPdfDocumentInit } from "../../pdfjsAssets";

import "../../pdf-worker";

function pageAncestor(el: Node | null): number | null {
  let cur: Node | null = el;
  while (cur && cur !== document.body) {
    if (cur instanceof HTMLElement && cur.dataset.pageNum) {
      const n = Number(cur.dataset.pageNum);
      return Number.isFinite(n) ? n : null;
    }
    cur = cur.parentNode;
  }
  return null;
}

type PageCanvasProps = {
  doc: PDFDocumentProxy;
  pageNum: number;
  scale: number;
};

function PageCanvas({ doc, pageNum, scale }: PageCanvasProps) {
  const wrapRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const textRef = useRef<HTMLDivElement>(null);
  const cleanupRef = useRef<{ layer?: TextLayer; task?: { cancel: () => void } }>({});

  useEffect(() => {
    let cancelled = false;
    const canvas = canvasRef.current;
    const textDiv = textRef.current;
    const wrap = wrapRef.current;
    if (!canvas || !textDiv || !wrap) return;

    (async () => {
      const page: PDFPageProxy = await doc.getPage(pageNum);
      if (cancelled) {
        page.cleanup();
        return;
      }
      const viewport = page.getViewport({ scale });
      canvas.width = viewport.width;
      canvas.height = viewport.height;
      textDiv.style.width = `${viewport.width}px`;
      textDiv.style.height = `${viewport.height}px`;
      wrap.style.width = `${viewport.width}px`;
      wrap.style.height = `${viewport.height}px`;

      const ctx = canvas.getContext("2d");
      if (!ctx) return;

      const task = page.render({ canvas, canvasContext: ctx, viewport });
      cleanupRef.current.task = task;
      await task.promise;
      if (cancelled) return;

      textDiv.replaceChildren();
      const textContent = await page.getTextContent({
        includeMarkedContent: false,
      });
      const textLayer = new TextLayer({
        textContentSource: textContent,
        container: textDiv,
        viewport,
      });
      cleanupRef.current.layer = textLayer;
      await textLayer.render();
    })().catch(console.error);

    return () => {
      cancelled = true;
      cleanupRef.current.task?.cancel();
      cleanupRef.current.layer?.cancel();
      cleanupRef.current = {};
    };
  }, [doc, pageNum, scale]);

  return (
    <div
      ref={wrapRef}
      className="pdf-page-wrap"
      data-page-num={String(pageNum)}
    >
      <canvas ref={canvasRef} className="pdf-page-canvas" />
      <div ref={textRef} className="textLayer" />
    </div>
  );
}

type PdfViewerProps = {
  fileUrl: string | null;
  onSelectionChange: (text: string, page: number | null) => void;
};

export function PdfViewer({ fileUrl, onSelectionChange }: PdfViewerProps) {
  const [doc, setDoc] = useState<PDFDocumentProxy | null>(null);
  const [numPages, setNumPages] = useState(0);
  const [scale] = useState(1.15);
  const scrollerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!fileUrl) {
      setDoc((prev) => {
        prev?.destroy();
        return null;
      });
      setNumPages(0);
      return;
    }
    let cancelled = false;
    const loading = getDocument(getPdfDocumentInit(fileUrl)).promise;
    loading
      .then((d) => {
        if (cancelled) {
          d.destroy();
          return;
        }
        setDoc(d);
        setNumPages(d.numPages);
      })
      .catch(console.error);
    return () => {
      cancelled = true;
    };
  }, [fileUrl]);

  const onMouseUp = useCallback(() => {
    const sel = window.getSelection();
    const text = sel?.toString().trim() ?? "";
    if (!text) {
      onSelectionChange("", null);
      return;
    }
    const anchor = sel?.anchorNode ?? null;
    const page = pageAncestor(anchor);
    onSelectionChange(text, page);
  }, [onSelectionChange]);

  if (!fileUrl) {
    return (
      <div className="pdf-empty">
        <p>「PDFを開く」から教科書を選択してください。</p>
      </div>
    );
  }

  if (!doc || numPages === 0) {
    return (
      <div className="pdf-empty">
        <p>読み込み中…</p>
      </div>
    );
  }

  return (
    <div
      ref={scrollerRef}
      className="pdf-scroller"
      onMouseUp={onMouseUp}
      role="document"
    >
      {Array.from({ length: numPages }, (_, i) => (
        <PageCanvas key={i + 1} doc={doc} pageNum={i + 1} scale={scale} />
      ))}
    </div>
  );
}
