import { useState } from "react";
import { AgentStatus } from "../../store/appStore";
import "./AgentPanel.css";

const AGENT_LABELS: Record<string, string> = {
  normalizer: "入力正規化",
  rag: "RAG 検索",
  memory: "Memory",
  context: "Context",
  main_agent: "Main Agent",
  adversarial: "Adversarial",
  compression: "圧縮",
  all: "完了",
};

const AGENT_ICONS: Record<string, string> = {
  normalizer: "✏️",
  rag: "🔍",
  memory: "🧠",
  context: "📋",
  main_agent: "💬",
  adversarial: "⚠️",
  compression: "💾",
  all: "✅",
};

type Props = {
  statuses: AgentStatus[];
  isActive: boolean;
};

export function AgentPanel({ statuses, isActive }: Props) {
  const [expanded, setExpanded] = useState(false);

  // "all" ステータスを除いた実エージェント
  const agentStatuses = statuses.filter((s) => s.agent !== "all");
  const allDone = statuses.some((s) => s.agent === "all" && s.status === "done");
  const hasAny = agentStatuses.length > 0;

  if (!hasAny && !isActive) return null;

  const doneCount = agentStatuses.filter((s) => s.status === "done").length;
  const runningAgent = agentStatuses.find((s) => s.status === "running");

  const summary = allDone
    ? `${doneCount}つのエージェントが動作しました`
    : runningAgent
      ? `${AGENT_LABELS[runningAgent.agent] ?? runningAgent.agent} 実行中...`
      : "エージェント待機中";

  return (
    <div className={`agent-panel ${allDone ? "done" : "running"}`}>
      <button
        className="agent-panel-header"
        onClick={() => setExpanded((v) => !v)}
      >
        <span className="agent-panel-icon">{allDone ? "✅" : "⚙️"}</span>
        <span className="agent-panel-summary">ℹ️ {summary}</span>
        <span className="agent-panel-chevron">{expanded ? "▲" : "▼"}</span>
      </button>

      {expanded && (
        <div className="agent-panel-details">
          {agentStatuses.map((s) => (
            <div
              key={s.agent}
              className={`agent-row agent-${s.status}`}
            >
              <span className="agent-icon">
                {s.status === "running"
                  ? "⏳"
                  : s.status === "error"
                    ? "❌"
                    : AGENT_ICONS[s.agent] ?? "✅"}
              </span>
              <span className="agent-name">
                {AGENT_LABELS[s.agent] ?? s.agent}
              </span>
              {s.detail && (
                <span className="agent-detail">{s.detail}</span>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
