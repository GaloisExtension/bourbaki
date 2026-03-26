import { memo, useMemo, useState } from "react";
import { Handle, Position, type Node, type NodeProps } from "@xyflow/react";
import katex from "katex";
import "katex/dist/katex.min.css";
import "./chat-node.css";

export type ChatNodeData = {
  title: string;
  selectionPreview: string;
  thinkingBadge: string;
  pageLabel: string;
  onSubmit?: (text: string) => void;
};

function tryKatexPreview(raw: string): string | null {
  const trimmed = raw.trim();
  if (!trimmed) return null;
  try {
    return katex.renderToString(trimmed, {
      throwOnError: false,
      displayMode: false,
    });
  } catch {
    return null;
  }
}

export const ChatNode = memo(function ChatNode({
  data,
}: NodeProps<Node<ChatNodeData>>) {
  const [text, setText] = useState("");
  const preview = useMemo(() => tryKatexPreview(text), [text]);

  return (
    <div className="chat-node">
      <Handle type="target" position={Position.Left} className="chat-handle" />
      <header className="chat-node__head">
        <span className="chat-node__title">{data.title}</span>
        <span className="chat-node__badge" title="Thinking モード">
          {data.thinkingBadge}
        </span>
      </header>
      <section className="chat-node__ctx">
        <div className="chat-node__ctx-meta">{data.pageLabel}</div>
        <pre className="chat-node__selection">{data.selectionPreview || "（選択なし）"}</pre>
      </section>
      <div className="chat-node__preview-row">
        {preview ? (
          <div
            className="chat-node__katex"
            dangerouslySetInnerHTML={{ __html: preview }}
          />
        ) : (
          <span className="chat-node__preview-placeholder">
            数式プレビュー（入力で表示）
          </span>
        )}
      </div>
      <textarea
        className="chat-node__input"
        placeholder="質問を入力（非形式なLaTeX可）…"
        value={text}
        rows={3}
        onChange={(e) => setText(e.target.value)}
      />
      <footer className="chat-node__foot">
        <button
          type="button"
          className="chat-node__btn chat-node__btn--primary"
          onClick={() => data.onSubmit?.(text)}
        >
          送信（Phase 3でLLM接続）
        </button>
      </footer>
      <Handle type="source" position={Position.Right} className="chat-handle" />
    </div>
  );
});
