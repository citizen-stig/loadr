// The visual canvas editor — a fourth way to build a plan, beside Form / Split
// / YAML. The plan becomes a node graph (scenarios → nested steps) with arrows
// for sequence and nesting; you pan, zoom, drag to arrange, drag a step from
// the palette onto a container to add it, drag an existing step onto another
// container to re-parent it, and edit the selected node in the right-hand
// inspector. Every structural or field edit goes through the shared PlanDoc,
// so the YAML, validation and this graph never drift.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  Background,
  BackgroundVariant,
  ControlButton,
  Controls,
  MarkerType,
  MiniMap,
  ReactFlow,
  ReactFlowProvider,
  useEdgesState,
  useNodesState,
  useReactFlow,
  type Edge,
  type Node,
  type NodeChange,
  type XYPosition,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';

import { addStepAt, moveStepBetween } from '../../shared/edit';
import type { StepKind } from '../../shared/types';
import type { PlanDoc } from '../state/usePlanDoc';
import { useSelection } from '../state/selection';
import { dropTargetPath, planToGraph, type NodeData, type PlanNode } from './graph';
import { Inspector } from './Inspector';
import { DND_MIME, Palette } from './Palette';
import { NODE_TYPES } from './nodes';

const SEQ = '#6b7280';
const NEST = '#3a3a46';

function styleEdges(edges: Edge[]): Edge[] {
  return edges.map((e) =>
    e.type === 'seq'
      ? { ...e, type: 'smoothstep', markerEnd: { type: MarkerType.ArrowClosed, color: SEQ }, style: { stroke: SEQ, strokeWidth: 1.5 } }
      : { ...e, type: 'smoothstep', animated: false, style: { stroke: NEST, strokeDasharray: '4 3' } },
  );
}

function CanvasInner({ doc }: { doc: PlanDoc }) {
  const { selectedId, select } = useSelection();
  const positions = useRef<Record<string, XYPosition>>({});
  const rf = useReactFlow();
  const [showMiniMap, setShowMiniMap] = useState(false); // off by default; toggled from the controls

  const graph = useMemo(() => planToGraph(doc.plan, positions.current), [doc.plan]);

  const [nodes, setNodes, onNodesChange] = useNodesState<PlanNode>(graph.nodes);
  const [edges, setEdges] = useEdgesState<Edge>(styleEdges(graph.edges));

  // Re-seed when the plan structure changes, preserving user-dragged positions
  // and the current selection highlight.
  useEffect(() => {
    setNodes(graph.nodes.map((n) => ({ ...n, selected: n.id === selectedId })));
    setEdges(styleEdges(graph.edges));
  }, [graph, selectedId, setNodes, setEdges]);

  // Capture drag positions so a plan edit doesn't reset the user's arrangement.
  const handleNodesChange = useCallback(
    (changes: NodeChange<PlanNode>[]) => {
      for (const c of changes) {
        if (c.type === 'position' && c.position) positions.current[c.id] = c.position;
      }
      onNodesChange(changes);
    },
    [onNodesChange],
  );

  const onNodeClick = useCallback((_: unknown, node: Node) => select(node.id), [select]);
  const onPaneClick = useCallback(() => select(''), [select]);

  // Deepest ordered container at a point (palette drop target).
  const containerAt = useCallback(
    (point: XYPosition, exclude?: string): PlanNode | null => {
      const hits = rf
        .getIntersectingNodes({ x: point.x, y: point.y, width: 1, height: 1 })
        .filter((n) => n.id !== exclude && dropTargetPath((n as PlanNode).data)) as PlanNode[];
      if (hits.length === 0) return null;
      return hits.reduce((a, b) => (b.data.depth > a.data.depth ? b : a));
    },
    [rf],
  );

  // Palette → canvas: add a new step of `kind` into the container dropped on.
  const onDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      const kind = e.dataTransfer.getData(DND_MIME) as StepKind;
      if (!kind) return;
      const point = rf.screenToFlowPosition({ x: e.clientX, y: e.clientY });
      const target = containerAt(point);
      const stepsPath = target && dropTargetPath(target.data);
      if (!stepsPath) return;
      doc.apply((p) => addStepAt(p, stepsPath, kind));
    },
    [rf, containerAt, doc],
  );

  // Drag an existing step onto another container → re-parent it there.
  const onNodeDragStop = useCallback(
    (_: unknown, node: Node) => {
      const data = (node as PlanNode).data;
      if (!data.listPath || data.index == null) return; // only steps re-parent
      const intersecting = rf
        .getIntersectingNodes(node)
        .filter((n) => n.id !== node.id && dropTargetPath((n as PlanNode).data)) as PlanNode[];
      if (intersecting.length === 0) return;
      const target = intersecting.reduce((a, b) => (b.data.depth > a.data.depth ? b : a));
      const toPath = dropTargetPath(target.data);
      if (!toPath) return;
      // No-op if it's already this step's own list.
      if (toPath.join('.') === data.listPath.join('.')) return;
      doc.apply((p) => moveStepBetween(p, data.listPath!, data.index!, toPath));
    },
    [rf, doc],
  );

  const selectedNode: NodeData | null = useMemo(
    () => graph.nodes.find((n) => n.id === selectedId)?.data ?? null,
    [graph.nodes, selectedId],
  );

  return (
    <div className="flex h-full min-h-0">
      <aside className="w-56 shrink-0 border-r border-edge">
        <Palette />
      </aside>

      <div
        className="relative min-w-0 flex-1"
        onDrop={onDrop}
        onDragOver={(e) => { e.preventDefault(); e.dataTransfer.dropEffect = 'copy'; }}
      >
        <ReactFlow
          nodes={nodes}
          edges={edges}
          nodeTypes={NODE_TYPES}
          onNodesChange={handleNodesChange}
          onNodeClick={onNodeClick}
          onPaneClick={onPaneClick}
          onNodeDragStop={onNodeDragStop}
          fitView
          minZoom={0.2}
          maxZoom={1.75}
          proOptions={{ hideAttribution: true }}
          className="bg-ink"
        >
          <Background variant={BackgroundVariant.Dots} gap={22} size={1} color={NEST} />
          <Controls className="!border-edge !bg-coal [&_button]:!border-edge [&_button]:!bg-panel [&_button]:!fill-ash [&_button:hover]:!bg-edge">
            <ControlButton
              onClick={() => setShowMiniMap((s) => !s)}
              title={showMiniMap ? 'Hide minimap' : 'Show minimap'}
              aria-label={showMiniMap ? 'Hide minimap' : 'Show minimap'}
              className={showMiniMap ? '!fill-flare' : ''}
            >
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <rect x="3" y="3" width="18" height="18" rx="2" /><rect x="13" y="13" width="6" height="6" rx="1" fill="currentColor" stroke="none" />
              </svg>
            </ControlButton>
          </Controls>
          {showMiniMap && (
            <MiniMap pannable zoomable className="!bg-coal" maskColor="rgba(0,0,0,0.6)" nodeColor="#2a2a35" />
          )}
        </ReactFlow>
      </div>

      <aside className="w-80 shrink-0 border-l border-edge">
        <Inspector node={selectedNode} doc={doc} onClose={() => select('')} onSelect={select} />
      </aside>
    </div>
  );
}

export function Canvas({ doc }: { doc: PlanDoc }) {
  return (
    <ReactFlowProvider>
      <CanvasInner doc={doc} />
    </ReactFlowProvider>
  );
}
