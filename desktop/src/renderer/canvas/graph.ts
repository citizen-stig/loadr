// Map a plan to a node graph for the canvas editor. Pure and headless-testable:
// no React, no React Flow runtime.
//
// The tree comes straight from `buildOutline`, so the canvas covers exactly
// what the outline does: Plan → scenarios → every nested step kind, with the
// same node ids (path.join('.')) the rest of the editor already uses for
// selection. Positions are a tidy left-to-right layout (x by depth, y by leaf
// order) that any plan can render into immediately.

import type { Edge, Node } from '@xyflow/react';

import type { Path } from '../../shared/edit';
import { buildOutline, type OutlineNode } from '../../shared/outline';
import type { Plan } from '../../shared/types';

export interface NodeData extends Record<string, unknown> {
  label: string;
  /** 'plan' | 'scenario' | a StepKind | 'sublist' */
  kind: string;
  summary?: string;
  /** Edit path into the plan model (drives the inspector + mutations). */
  path: Path;
  /** True when this node's children are an *ordered* steps list (draw the
      sequence arrows among them). False for scenarios / alternative branches. */
  ordered: boolean;
  depth: number;
  /** When this node is an element of an ordered steps list: that list's path
      and this node's index in it — the coordinates reorder/reparent needs. */
  listPath?: Path;
  index?: number;
}

export type PlanNode = Node<NodeData>;
export type PlanEdge = Edge;

export const COL_W = 300;
export const ROW_H = 92;

/** Children of these node kinds form an ordered steps list. */
function isOrdered(kind: string): boolean {
  return kind === 'scenario' || kind === 'sublist';
}

function nodeType(kind: string): string {
  if (kind === 'plan') return 'plan';
  if (kind === 'scenario') return 'scenario';
  if (kind === 'sublist') return 'sublist';
  return 'step';
}

/**
 * Build the full graph for a plan.
 *
 * `positions` overrides layout for nodes the user has dragged (keyed by node
 * id); anything missing falls back to the tidy layout, so a freshly-opened or
 * newly-restructured plan always lays itself out.
 */
export function planToGraph(
  plan: Plan,
  positions: Record<string, { x: number; y: number }> = {},
): { nodes: PlanNode[]; edges: PlanEdge[] } {
  const outline = buildOutline(plan);
  if (outline.length === 0) return { nodes: [], edges: [] };

  // buildOutline returns [planNode, ...scenarioNodes] flat; nest the scenarios
  // under the plan so the graph has a single root.
  const root: OutlineNode = { ...outline[0], children: outline.slice(1) };

  // Tidy layout: x = depth * COL_W; y from an incrementing leaf counter, with
  // each parent centred on the span of its children.
  const pos = new Map<string, { x: number; y: number }>();
  let leaf = 0;
  const place = (node: OutlineNode, depth: number): number => {
    const x = depth * COL_W;
    let y: number;
    if (node.children.length === 0) {
      y = leaf++ * ROW_H;
    } else {
      const ys = node.children.map((c) => place(c, depth + 1));
      y = (ys[0] + ys[ys.length - 1]) / 2;
    }
    pos.set(node.id, { x, y });
    return y;
  };
  place(root, 0);

  const nodes: PlanNode[] = [];
  const edges: PlanEdge[] = [];

  const emit = (node: OutlineNode, depth: number, parentId: string | null): void => {
    const last = node.path[node.path.length - 1];
    const isListElement = typeof last === 'number';
    nodes.push({
      id: node.id,
      type: nodeType(node.kind),
      position: positions[node.id] ?? pos.get(node.id) ?? { x: 0, y: 0 },
      data: {
        label: node.label,
        kind: node.kind,
        summary: node.summary,
        path: node.path,
        ordered: isOrdered(node.kind),
        depth,
        ...(isListElement ? { listPath: node.path.slice(0, -1), index: last as number } : {}),
      },
    });

    // Nesting edge: parent contains child (right handle → left handle).
    if (parentId) {
      edges.push({
        id: `nest:${parentId}->${node.id}`,
        source: parentId,
        target: node.id,
        sourceHandle: 'r',
        targetHandle: 'l',
        type: 'nest',
      });
    }

    const ordered = isOrdered(node.kind);
    node.children.forEach((child, i) => {
      emit(child, depth + 1, node.id);
      // Sequence arrow between consecutive steps of an ordered list
      // (bottom handle → top handle).
      if (ordered && i > 0) {
        const prev = node.children[i - 1];
        edges.push({
          id: `seq:${prev.id}->${child.id}`,
          source: prev.id,
          target: child.id,
          sourceHandle: 'b',
          targetHandle: 't',
          type: 'seq',
        });
      }
    });
  };

  emit(root, 0, null);
  return { nodes, edges };
}

/**
 * The steps-array path a node accepts children into — for palette drops and
 * drag-to-reparent. A scenario holds its `flow`; a sublist (then/else, a
 * switch case, a parallel branch, a group's steps) *is* a steps array. Any
 * other node isn't a drop target.
 */
export function dropTargetPath(data: NodeData): Path | null {
  if (data.kind === 'scenario') return [...data.path, 'flow'];
  if (data.kind === 'sublist') return data.path;
  return null;
}
