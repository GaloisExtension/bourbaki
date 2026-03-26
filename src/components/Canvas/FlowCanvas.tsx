import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Background,
  Controls,
  MiniMap,
  ReactFlow,
  ReactFlowProvider,
  type Edge,
  type Node,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { ChatNode, type ChatNodeData } from "./ChatNode";
import { useAppStore } from "../../store/appStore";
import type { ChatMessageRow, SessionDetail } from "../../store/appStore";
import {
  branchSession,
  createSession,
  finalizeSessionMemory,
  listSessionMessages,
  sendSessionMessage,
  setSessionResolved,
} from "../../api/commands";

const types = { chat: ChatNode };

function layoutTree(sessions: SessionDetail[]): Map<string, { x: number; y: number }> {
  const ROOT = "__root__";
  const byParent = new Map<string, string[]>();
  for (const s of sessions) {
    const p = s.parentId ?? ROOT;
    if (!byParent.has(p)) byParent.set(p, []);
    byParent.get(p)!.push(s.id);
  }
  const positions = new Map<string, { x: number; y: number }>();
  let yCursor = 50;
  const roots = byParent.get(ROOT) ?? [];

  function subtree(sid: string, depth: number, y: number): number {
    positions.set(sid, { x: 36 + depth * 430, y });
    const kids = byParent.get(sid) ?? [];
    let next = y + 48;
    for (const k of kids) {
      next = subtree(k, depth + 1, next);
      next += 24;
    }
    return Math.max(y + 300, next);
  }

  for (const rid of roots) {
    yCursor = subtree(rid, 0, yCursor);
    yCursor += 56;
  }
  return positions;
}

export function FlowCanvas() {
  const thinkingEnabled = useAppStore((s) => s.thinkingEnabled);
  const sessionRows = useAppStore((s) => s.sessionRows);
  const chatVersion = useAppStore((s) => s.chatVersion);
  const selectionText = useAppStore((s) => s.selectionText);
  const selectionPage = useAppStore((s) => s.selectionPage);
  const selectionLatexMapped = useAppStore((s) => s.selectionLatexMapped);
  const agentStatuses = useAppStore((s) => s.agentStatuses);
  const clearAgentStatuses = useAppStore((s) => s.clearAgentStatuses);

  const [messagesBySession, setMessagesBySession] = useState<
    Record<string, ChatMessageRow[]>
  >({});
  const [sendingId, setSendingId] = useState<string | null>(null);
  const [errMap, setErrMap] = useState<Record<string, string>>({});

  useEffect(() => {
    if (sessionRows.length === 0) {
      setMessagesBySession({});
      return;
    }
    let cancelled = false;
    void Promise.all(
      sessionRows.map((s) =>
        listSessionMessages(s.id).then(
          (rows) =>
            [
              s.id,
              rows.map((r) => ({
                id: r.id,
                role: r.role,
                content: r.content,
                createdAt: r.createdAt,
              })),
            ] as const,
        ),
      ),
    ).then((entries) => {
      if (!cancelled) {
        setMessagesBySession(Object.fromEntries(entries));
      }
    });
    return () => {
      cancelled = true;
    };
  }, [sessionRows, chatVersion]);

  const onSend = useCallback(
    async (sessionId: string | null, text: string) => {
      setErrMap({});
      clearAgentStatuses();
      const st = useAppStore.getState();
      try {
        let sid = sessionId;
        if (!sid) {
          setSendingId("__draft__");
          sid = await createSession({
            bookId: st.bookId,
            pageNum: st.selectionPage,
            selectionText: st.selectionText || null,
            selectionLatex: st.selectionLatexMapped || null,
            parentId: null,
          });
        } else {
          setSendingId(sid);
        }
        await sendSessionMessage({
          sessionId: sid,
          userText: text,
          thinkingEnabled: st.thinkingEnabled,
        });
        st.bumpChatVersion();
      } catch (e) {
        const key = sessionId ?? "__draft__";
        setErrMap({ [key]: String(e) });
      } finally {
        setSendingId(null);
      }
    },
    [clearAgentStatuses],
  );

  const onBranch = useCallback(async (parentId: string) => {
    try {
      await branchSession(parentId);
      useAppStore.getState().bumpChatVersion();
    } catch (e) {
      setErrMap((m) => ({ ...m, [parentId]: String(e) }));
    }
  }, []);

  const onResolved = useCallback(async (sessionId: string, resolved: boolean) => {
    try {
      if (resolved) {
        await finalizeSessionMemory(sessionId);
      } else {
        await setSessionResolved(sessionId, false);
      }
      useAppStore.getState().bumpChatVersion();
    } catch (e) {
      setErrMap((m) => ({ ...m, [sessionId]: String(e) }));
    }
  }, []);

  const positions = useMemo(
    () => layoutTree(sessionRows),
    [sessionRows],
  );

  const nodes: Node<ChatNodeData>[] = useMemo(() => {
    const rows: Node<ChatNodeData>[] = sessionRows.map((s) => ({
      id: s.id,
      type: "chat",
      position: positions.get(s.id) ?? { x: 40, y: 60 },
      data: {
        sessionId: s.id,
        title: `スレッド`,
        selectionPreview: s.selectionText ?? "",
        latexMappedSnippet: s.selectionLatex ?? "",
        thinkingBadge: thinkingEnabled ? "🧠" : "⚡",
        pageLabel:
          s.pageNum != null ? `選択ページ: ${s.pageNum}` : "選択ページ: —",
        resolved: s.resolved,
        messages: messagesBySession[s.id] ?? [],
        chatError: errMap[s.id] ?? null,
        sending: sendingId === s.id,
        agentStatuses: sendingId === s.id ? agentStatuses : [],
        onSend,
        onBranch,
        onResolved,
      },
    }));

    if (sessionRows.length === 0) {
      rows.push({
        id: "__draft__",
        type: "chat",
        position: { x: 40, y: 60 },
        data: {
          sessionId: null,
          title: "新規スレッド",
          selectionPreview: selectionText,
          latexMappedSnippet: selectionLatexMapped ?? "",
          thinkingBadge: thinkingEnabled ? "🧠" : "⚡",
          pageLabel:
            selectionPage != null
              ? `選択ページ: ${selectionPage}`
              : "選択ページ: —",
          resolved: false,
          messages: [],
          chatError: errMap["__draft__"] ?? null,
          sending: sendingId === "__draft__",
          agentStatuses: sendingId === "__draft__" ? agentStatuses : [],
          onSend,
          onBranch,
          onResolved,
        },
      });
    }
    return rows;
  }, [
    sessionRows,
    positions,
    messagesBySession,
    errMap,
    sendingId,
    thinkingEnabled,
    selectionText,
    selectionLatexMapped,
    selectionPage,
    agentStatuses,
    onSend,
    onBranch,
    onResolved,
  ]);

  const edges: Edge[] = useMemo(
    () =>
      sessionRows
        .filter((s) => s.parentId)
        .map((s) => ({
          id: `${s.parentId}-${s.id}`,
          source: s.parentId!,
          target: s.id,
        })),
    [sessionRows],
  );

  const nodeTypes = useMemo(() => types, []);
  const isValidConnection = useCallback(() => false, []);

  return (
    <div className="flow-host">
      <ReactFlowProvider>
        <ReactFlow
          nodes={nodes}
          edges={edges}
          nodeTypes={nodeTypes}
          fitView
          fitViewOptions={{ padding: 0.2 }}
          minZoom={0.35}
          maxZoom={1.4}
          nodesDraggable={false}
          proOptions={{ hideAttribution: true }}
          isValidConnection={isValidConnection}
        >
          <Background gap={20} size={1} color="#334155" />
          <Controls />
          <MiniMap
            nodeStrokeWidth={2}
            maskColor="rgba(15, 23, 42, 0.85)"
            style={{ background: "#0f172a" }}
          />
        </ReactFlow>
      </ReactFlowProvider>
    </div>
  );
}
