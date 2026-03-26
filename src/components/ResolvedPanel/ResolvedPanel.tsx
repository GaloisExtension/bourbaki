import { useCallback, useEffect, useState } from "react";
import { listResolvedSessions, setSessionResolved } from "../../api/commands";
import "./ResolvedPanel.css";

type ResolvedSession = {
  id: string;
  pageNum: number | null;
  selectionText: string | null;
  createdAt: number;
  summary: string | null;
  resolvedAt: number | null;
};

type Props = {
  bookId: string;
  onFocusSession?: (sessionId: string) => void;
  onUnresolved?: () => void;
};

function fmtDate(unix: number | null): string {
  if (!unix) return "—";
  return new Date(unix * 1000).toLocaleDateString("ja-JP", {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function ResolvedPanel({ bookId, onFocusSession, onUnresolved }: Props) {
  const [open, setOpen] = useState(false);
  const [sessions, setSessions] = useState<ResolvedSession[]>([]);
  const [filter, setFilter] = useState("");
  const [loading, setLoading] = useState(false);
  const [expanded, setExpanded] = useState<string | null>(null);

  const refresh = useCallback(() => {
    if (!bookId) return;
    setLoading(true);
    listResolvedSessions(bookId)
      .then(setSessions)
      .catch(console.error)
      .finally(() => setLoading(false));
  }, [bookId]);

  useEffect(() => {
    if (open) refresh();
  }, [open, refresh]);

  const handleUnresolve = useCallback(
    async (sessionId: string) => {
      await setSessionResolved(sessionId, false);
      refresh();
      onUnresolved?.();
    },
    [refresh, onUnresolved],
  );

  const filtered = sessions.filter((s) => {
    if (!filter.trim()) return true;
    const q = filter.toLowerCase();
    return (
      s.selectionText?.toLowerCase().includes(q) ||
      s.summary?.toLowerCase().includes(q) ||
      String(s.pageNum).includes(q)
    );
  });

  return (
    <div className={`resolved-panel ${open ? "resolved-panel--open" : ""}`}>
      <button
        className="resolved-panel__toggle"
        onClick={() => setOpen((v) => !v)}
        title="解決済みセッション一覧"
      >
        <span className="resolved-panel__toggle-icon">✅</span>
        {open ? "◀" : "▶"}
      </button>

      {open && (
        <div className="resolved-panel__body">
          <div className="resolved-panel__head">
            <span className="resolved-panel__title">解決済み一覧</span>
            <span className="resolved-panel__count">{sessions.length}件</span>
          </div>

          <input
            className="resolved-panel__search"
            type="search"
            placeholder="キーワードで絞り込み…"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />

          <div className="resolved-panel__list">
            {loading ? (
              <div className="resolved-panel__empty">読み込み中…</div>
            ) : filtered.length === 0 ? (
              <div className="resolved-panel__empty">
                {sessions.length === 0 ? "まだ解決済みセッションがありません" : "該当なし"}
              </div>
            ) : (
              filtered.map((s) => (
                <div
                  key={s.id}
                  className={`resolved-item ${expanded === s.id ? "resolved-item--expanded" : ""}`}
                >
                  <button
                    className="resolved-item__header"
                    onClick={() => setExpanded(expanded === s.id ? null : s.id)}
                  >
                    <span className="resolved-item__page">
                      {s.pageNum != null ? `p.${s.pageNum}` : "—"}
                    </span>
                    <span className="resolved-item__preview">
                      {s.selectionText?.slice(0, 40) ?? "（選択なし）"}
                      {(s.selectionText?.length ?? 0) > 40 ? "…" : ""}
                    </span>
                    <span className="resolved-item__date">{fmtDate(s.resolvedAt)}</span>
                  </button>

                  {expanded === s.id && (
                    <div className="resolved-item__detail">
                      {s.summary ? (
                        <pre className="resolved-item__summary">{s.summary}</pre>
                      ) : (
                        <div className="resolved-item__no-summary">サマリーなし</div>
                      )}
                      <div className="resolved-item__actions">
                        {onFocusSession && (
                          <button
                            className="resolved-item__btn"
                            onClick={() => onFocusSession(s.id)}
                          >
                            キャンバスで表示
                          </button>
                        )}
                        <button
                          className="resolved-item__btn resolved-item__btn--danger"
                          onClick={() => handleUnresolve(s.id)}
                        >
                          未解決に戻す
                        </button>
                      </div>
                    </div>
                  )}
                </div>
              ))
            )}
          </div>
        </div>
      )}
    </div>
  );
}
