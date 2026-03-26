import { useCallback, useMemo } from "react";
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

const types = { chat: ChatNode };

type FlowCanvasProps = {
  onChatSubmit?: (text: string) => void;
};

export function FlowCanvas({ onChatSubmit }: FlowCanvasProps) {
  const selectionText = useAppStore((s) => s.selectionText);
  const selectionPage = useAppStore((s) => s.selectionPage);
  const thinkingEnabled = useAppStore((s) => s.thinkingEnabled);

  const nodes: Node<ChatNodeData>[] = useMemo(
    () => [
      {
        id: "root",
        type: "chat",
        position: { x: 40, y: 60 },
        data: {
          title: "ブランチ会話",
          selectionPreview: selectionText,
          thinkingBadge: thinkingEnabled ? "🧠" : "⚡",
          pageLabel:
            selectionPage != null
              ? `選択ページ: ${selectionPage}`
              : "選択ページ: —",
          onSubmit: onChatSubmit,
        },
      },
    ],
    [selectionText, selectionPage, thinkingEnabled, onChatSubmit],
  );

  const edges: Edge[] = useMemo(() => [], []);

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
          minZoom={0.4}
          maxZoom={1.6}
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
