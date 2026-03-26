import { memo, useMemo, useState } from "react";
import { Handle, Position, type Node, type NodeProps } from "@xyflow/react";
import katex from "katex";
import "katex/dist/katex.min.css";
import "./chat-node.css";
import { MathMessage } from "../Chat/MathMessage";
import { AgentPanel } from "../AgentPanel/AgentPanel";
import type { AgentStatus, ChatMessageRow } from "../../store/appStore";

export type ChatNodeData = {
  sessionId: string | null;
  title: string;
  selectionPreview: string;
  latexMappedSnippet: string;
  thinkingBadge: string;
  pageLabel: string;
  resolved: boolean;
  messages: ChatMessageRow[];
  chatError: string | null;
  sending: boolean;
  agentStatuses: AgentStatus[];
  onSend: (sessionId: string | null, text: string) => void;
  onBranch?: (sessionId: string) => void;
  onResolved?: (sessionId: string, resolved: boolean) => void;
};

function tryKatexPreview(raw: string): string | null {
  const trimmed = raw.trim();
  if (!trimmed) return null;
  try {
    return katex.renderToString(trimmed, {
      throwOnError: false,
      strict: "ignore",
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

  const canBranch = Boolean(data.sessionId && data.onBranch);
  const canResolve = Boolean(data.sessionId && data.onResolved);

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
        {data.latexMappedSnippet ? (
          <div className="chat-node__latex-map">
            <div className="chat-node__ctx-meta">LaTeX 対応（推定・取り込み後）</div>
            <pre className="chat-node__selection chat-node__selection--latexmap">
              {data.latexMappedSnippet}
            </pre>
          </div>
        ) : null}
      </section>

      <div className="chat-node__messages">
        {data.messages.length === 0 ? (
          <div className="chat-node__ctx-meta">メッセージはまだありません</div>
        ) : (
          data.messages.map((m) => (
            <div
              key={m.id}
              className={
                m.role === "user"
                  ? "chat-node__msg chat-node__msg--user"
                  : "chat-node__msg chat-node__msg--assistant"
              }
            >
              <div className="chat-node__msg-role">
                {m.role === "user" ? "あなた" : "チューター"}
              </div>
              {m.role === "assistant" ? (
                <MathMessage content={m.content} />
              ) : (
                <div className="math-msg__text">{m.content}</div>
              )}
            </div>
          ))
        )}
      </div>

      {data.chatError ? (
        <div className="chat-node__err">{data.chatError}</div>
      ) : null}

      <AgentPanel statuses={data.agentStatuses} isActive={data.sending} />

      <div className="chat-node__preview-row">
        {preview ? (
          <div
            className="chat-node__katex"
            dangerouslySetInnerHTML={{ __html: preview }}
          />
        ) : (
          <span className="chat-node__preview-placeholder">
            入力プレビュー（KaTeX）
          </span>
        )}
      </div>
      <textarea
        className="chat-node__input"
        placeholder="質問を入力（非形式なLaTeX可）…"
        value={text}
        rows={3}
        disabled={data.sending}
        onChange={(e) => setText(e.target.value)}
      />
      <footer className="chat-node__foot">
        <button
          type="button"
          className="chat-node__btn chat-node__btn--primary"
          disabled={data.sending || !text.trim()}
          onClick={() => {
            const t = text.trim();
            if (!t) return;
            data.onSend(data.sessionId, t);
            setText("");
          }}
        >
          {data.sending ? "送信中…" : "送信"}
        </button>
        {data.sessionId ? (
          <div className="chat-node__actions">
            {canBranch ? (
              <button
                type="button"
                className="chat-node__btn chat-node__btn--ghost"
                disabled={data.sending}
                onClick={() => data.sessionId && data.onBranch?.(data.sessionId)}
              >
                このスレッドから分岐
              </button>
            ) : null}
            {canResolve ? (
              <>
                <button
                  type="button"
                  className="chat-node__btn chat-node__btn--ghost"
                  disabled={data.sending || data.resolved}
                  onClick={() =>
                    data.sessionId && data.onResolved?.(data.sessionId, true)
                  }
                >
                  解決済み
                </button>
                <button
                  type="button"
                  className="chat-node__btn chat-node__btn--ghost"
                  disabled={data.sending || !data.resolved}
                  onClick={() =>
                    data.sessionId && data.onResolved?.(data.sessionId, false)
                  }
                >
                  未解決に戻す
                </button>
              </>
            ) : null}
          </div>
        ) : null}
      </footer>
      <Handle type="source" position={Position.Right} className="chat-handle" />
    </div>
  );
});
