// Inline, dependency-free SVG icons (lucide-style, 1.5px stroke). Inlined rather
// than pulled from an icon font / CDN so the renderer keeps its strict
// `default-src 'self'` CSP and works fully offline. Every icon inherits
// `currentColor` and sizes to 1em so it tracks the surrounding text.

import type { SVGProps } from 'react';

function Svg({ children, ...props }: SVGProps<SVGSVGElement>) {
  return (
    <svg
      width="1em"
      height="1em"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.75}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      {...props}
    >
      {children}
    </svg>
  );
}

export type Icon = (props: SVGProps<SVGSVGElement>) => React.ReactElement;

export const Plus: Icon = (p) => <Svg {...p}><path d="M12 5v14M5 12h14" /></Svg>;
export const Trash: Icon = (p) => <Svg {...p}><path d="M3 6h18M8 6V4h8v2M19 6l-1 14H6L5 6M10 11v5M14 11v5" /></Svg>;
export const Grip: Icon = (p) => <Svg {...p}><circle cx="9" cy="6" r="1" /><circle cx="9" cy="12" r="1" /><circle cx="9" cy="18" r="1" /><circle cx="15" cy="6" r="1" /><circle cx="15" cy="12" r="1" /><circle cx="15" cy="18" r="1" /></Svg>;
export const ChevronDown: Icon = (p) => <Svg {...p}><path d="m6 9 6 6 6-6" /></Svg>;
export const ChevronRight: Icon = (p) => <Svg {...p}><path d="m9 6 6 6-6 6" /></Svg>;
export const Play: Icon = (p) => <Svg {...p}><path d="M6 4l14 8-14 8V4z" /></Svg>;
export const Stop: Icon = (p) => <Svg {...p}><rect x="6" y="6" width="12" height="12" rx="2" /></Svg>;
export const Save: Icon = (p) => <Svg {...p}><path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z" /><path d="M17 21v-8H7v8M7 3v5h8" /></Svg>;
export const FolderOpen: Icon = (p) => <Svg {...p}><path d="M3 7a2 2 0 0 1 2-2h4l2 2h6a2 2 0 0 1 2 2v1M3 7v11a2 2 0 0 0 2 2h12.5a2 2 0 0 0 1.9-1.4L22 11H6.5a2 2 0 0 0-1.9 1.4L3 18" /></Svg>;
export const Import: Icon = (p) => <Svg {...p}><path d="M12 3v12m0 0 4-4m-4 4-4-4M4 17v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2" /></Svg>;
export const Copy: Icon = (p) => <Svg {...p}><rect x="9" y="9" width="12" height="12" rx="2" /><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" /></Svg>;
export const Puzzle: Icon = (p) => <Svg {...p}><path d="M14 3a2 2 0 0 0-4 0v1H7a1 1 0 0 0-1 1v3H5a2 2 0 0 0 0 4h1v3a1 1 0 0 0 1 1h3v1a2 2 0 0 0 4 0v-1h3a1 1 0 0 0 1-1v-3h1a2 2 0 0 0 0-4h-1V5a1 1 0 0 0-1-1h-3V3z" /></Svg>;
export const X: Icon = (p) => <Svg {...p}><path d="M18 6 6 18M6 6l12 12" /></Svg>;
export const Code: Icon = (p) => <Svg {...p}><path d="m16 18 4-6-4-6M8 6l-4 6 4 6" /></Svg>;
export const Rows: Icon = (p) => <Svg {...p}><rect x="3" y="4" width="18" height="7" rx="1.5" /><rect x="3" y="13" width="18" height="7" rx="1.5" /></Svg>;
export const Columns: Icon = (p) => <Svg {...p}><rect x="3" y="4" width="8" height="16" rx="1.5" /><rect x="13" y="4" width="8" height="16" rx="1.5" /></Svg>;
export const Check: Icon = (p) => <Svg {...p}><path d="M20 6 9 17l-5-5" /></Svg>;
export const Alert: Icon = (p) => <Svg {...p}><path d="M12 9v4m0 4h.01M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z" /></Svg>;
export const ArrowUp: Icon = (p) => <Svg {...p}><path d="M12 19V5m-7 7 7-7 7 7" /></Svg>;
export const ArrowDown: Icon = (p) => <Svg {...p}><path d="M12 5v14m7-7-7 7-7-7" /></Svg>;
export const Search: Icon = (p) => <Svg {...p}><circle cx="11" cy="11" r="7" /><path d="m21 21-4.3-4.3" /></Svg>;
export const PanelLeft: Icon = (p) => <Svg {...p}><rect x="3" y="4" width="18" height="16" rx="2" /><path d="M9 4v16" /></Svg>;
export const Sparkles: Icon = (p) => <Svg {...p}><path d="M12 3l1.6 4.6L18 9l-4.4 1.4L12 15l-1.6-4.6L6 9l4.4-1.4L12 3zM19 14l.8 2.2L22 17l-2.2.8L19 20l-.8-2.2L16 17l2.2-.8L19 14z" /></Svg>;
export const Key: Icon = (p) => <Svg {...p}><circle cx="7.5" cy="15.5" r="3.5" /><path d="m10 13 7-7M21 4l-3 3M17 8l2 2" /></Svg>;

// ---- step-kind glyphs ------------------------------------------------------
const Globe: Icon = (p) => <Svg {...p}><circle cx="12" cy="12" r="9" /><path d="M3 12h18M12 3a14 14 0 0 1 0 18 14 14 0 0 1 0-18z" /></Svg>;
const Clock: Icon = (p) => <Svg {...p}><circle cx="12" cy="12" r="9" /><path d="M12 7v5l3 2" /></Svg>;
const Braces: Icon = (p) => <Svg {...p}><path d="M8 3c-2 0-3 1-3 3v2c0 1-1 2-2 2 1 0 2 1 2 2v2c0 2 1 3 3 3M16 3c2 0 3 1 3 3v2c0 1 1 2 2 2-1 0-2 1-2 2v2c0 2-1 3-3 3" /></Svg>;
export const Layers: Icon = (p) => <Svg {...p}><path d="m12 3 9 5-9 5-9-5 9-5zM3 13l9 5 9-5M3 17l9 5 9-5" /></Svg>;
const Repeat: Icon = (p) => <Svg {...p}><path d="m17 2 4 4-4 4M3 11V9a4 4 0 0 1 4-4h14M7 22l-4-4 4-4M21 13v2a4 4 0 0 1-4 4H3" /></Svg>;
const Loop: Icon = (p) => <Svg {...p}><path d="M21 12a9 9 0 1 1-3-6.7M21 4v4h-4" /></Svg>;
const Branch: Icon = (p) => <Svg {...p}><circle cx="6" cy="6" r="2.5" /><circle cx="6" cy="18" r="2.5" /><circle cx="18" cy="8" r="2.5" /><path d="M6 8.5v7M6 13a6 6 0 0 1 6-6h3" /></Svg>;
const Shuffle: Icon = (p) => <Svg {...p}><path d="M16 3h5v5M4 20 21 3M21 16v5h-5M15 15l6 6M4 4l5 5" /></Svg>;
const ListOrdered: Icon = (p) => <Svg {...p}><path d="M10 6h11M10 12h11M10 18h11M4 6h1v4M4 10h2M4 15h1.5a1 1 0 0 1 .5 1.9L4 18h2" /></Svg>;
const Timer: Icon = (p) => <Svg {...p}><path d="M10 2h4M12 8v5l3 2" /><circle cx="12" cy="13" r="8" /></Svg>;
const Rotate: Icon = (p) => <Svg {...p}><path d="M21 12a9 9 0 1 1-2.6-6.4M21 3v5h-5" /></Svg>;
const Split: Icon = (p) => <Svg {...p}><path d="M16 3h5v5M21 3l-7 7M8 21H3v-5M3 21l7-7" /></Svg>;
const Users: Icon = (p) => <Svg {...p}><path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" /><circle cx="9" cy="7" r="4" /><path d="M22 21v-2a4 4 0 0 0-3-3.9M16 3.1a4 4 0 0 1 0 7.8" /></Svg>;

export const STEP_ICON: Record<string, Icon> = {
  request: Globe,
  think_time: Clock,
  js: Braces,
  group: Layers,
  repeat: Repeat,
  while: Loop,
  if: Branch,
  random: Shuffle,
  foreach: ListOrdered,
  switch: Branch,
  during: Timer,
  retry: Rotate,
  parallel: Split,
  rendezvous: Users,
};
