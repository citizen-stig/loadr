import { useCallback, useEffect, useRef, useState } from 'react';

import { AiPanel } from './AiPanel';
import { Editor, type EditorState } from './Editor';
import { PayloadLab } from './PayloadLab';
import { PluginsPanel } from './PluginsPanel';
import {
  activeTab, closeTab, emptyWorkspace, newTabId, openTab, selectTab, updateTab, type Workspace,
} from './workspace/tabs';
import { Badge, Button } from './ui/controls';
import { Copy, FolderOpen, Import, Layers, Plus, Puzzle, Sparkles, X } from './ui/icons';

const STARTER = `name: new plan
scenarios:
  default:
    executor: constant-vus
    vus: 1
    duration: 30s
    flow:
      - request:
          method: GET
          url: https://example.com
`;

interface Seed {
  content: string;
  path: string | null;
}

const basename = (p: string) => p.split(/[\\/]/).pop() || p;

// M3: the tabbed multi-plan workspace. Each tab owns an Editor (kept mounted so
// switching preserves edits); the tab bar reflects dirty state and title. New /
// Open / Import (via `loadr convert`) / Duplicate create tabs.
export default function App() {
  const [ws, setWs] = useState<Workspace>(() => openTab(emptyWorkspace, { id: newTabId(), title: 'untitled' }));
  const [seeds, setSeeds] = useState<Record<string, Seed>>(() => {
    const first = ws.tabs[0].id;
    return { [first]: { content: STARTER, path: null } };
  });
  const yamlRef = useRef<Record<string, string>>({});
  const [version, setVersion] = useState('');
  const [engineProblem, setEngineProblem] = useState<string | null>(null);
  const [showPlugins, setShowPlugins] = useState(false);
  const [showPayload, setShowPayload] = useState(false);
  const [showAi, setShowAi] = useState(false);

  useEffect(() => {
    // One health check on startup: a broken engine (missing binary, wrong CPU
    // arch, no exec permission) is diagnosed once, up front, with a plain-English
    // fix — instead of surfacing a cryptic spawn errno on the first run.
    window.loadr
      ?.doctor()
      .then((h) => {
        setVersion(h.version ?? 'loadr unavailable');
        setEngineProblem(h.ok ? null : (h.problem ?? 'The loadr engine could not be started.'));
      })
      .catch((e: Error) => {
        setVersion('loadr unavailable');
        setEngineProblem(e.message);
      });
  }, []);

  const report = useCallback((id: string, s: EditorState) => {
    yamlRef.current[id] = s.yaml;
    setWs((prev) => {
      const t = prev.tabs.find((x) => x.id === id);
      if (!t) return prev;
      const title = s.path ? basename(s.path) : t.title;
      if (t.dirty === s.dirty && t.path === s.path && t.title === title) return prev;
      return updateTab(prev, id, { dirty: s.dirty, path: s.path, title });
    });
  }, []);

  function newTab(content = STARTER, path: string | null = null, title = 'untitled') {
    const id = newTabId();
    setSeeds((s) => ({ ...s, [id]: { content, path } }));
    setWs((prev) => openTab(prev, { id, path, title }));
  }

  async function openFile() {
    const o = await window.loadr.openPlan();
    if (o) newTab(o.content, o.path, basename(o.path));
  }

  async function importFile() {
    const o = await window.loadr.importPlan();
    if (o) newTab(o.content, null, `${basename(o.path)} (imported)`);
  }

  function duplicate() {
    const at = activeTab(ws);
    if (at) newTab(yamlRef.current[at.id] ?? seeds[at.id]?.content ?? STARTER, null, `${at.title} copy`);
  }

  return (
    <div className="flex h-screen flex-col bg-ink text-ash">
      <header className="flex items-center justify-between border-b border-edge bg-coal px-4 py-2.5">
        <div className="flex items-center gap-2.5">
          <span className="grid h-7 w-7 place-items-center rounded-lg bg-ink">
            <img src="data:image/png;base64,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" className="h-5 w-5" alt="" />
          </span>
          <span className="text-[15px] font-extrabold tracking-tight text-white">
            loadr <span className="font-medium text-smoke">Desktop</span>
          </span>
          <Badge tone="ember">Beta</Badge>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="primary" icon={Sparkles} onClick={() => setShowAi(true)}>AI</Button>
          <div className="mx-1 h-5 w-px bg-edge" />
          <Button icon={Plus} onClick={() => newTab()}>New</Button>
          <Button icon={FolderOpen} onClick={openFile}>Open…</Button>
          <Button icon={Import} onClick={importFile}>Import…</Button>
          <Button icon={Copy} onClick={duplicate}>Duplicate</Button>
          <div className="mx-1 h-5 w-px bg-edge" />
          <Button icon={Puzzle} onClick={() => setShowPlugins(true)}>Plugins</Button>
          <Button icon={Layers} onClick={() => setShowPayload(true)}>Payload Lab</Button>
        </div>
      </header>

      {engineProblem && (
        <div
          role="alert"
          className="flex items-start gap-2 border-b border-blood/40 bg-blood/10 px-4 py-2 text-sm text-flare"
        >
          <span aria-hidden className="mt-0.5 font-bold">⚠</span>
          <div>
            <span className="font-semibold">loadr engine problem.</span>{' '}
            <span className="text-ash">{engineProblem}</span>
          </div>
        </div>
      )}

      {showPlugins && <PluginsPanel onClose={() => setShowPlugins(false)} />}
      {showPayload && <PayloadLab onClose={() => setShowPayload(false)} />}
      {showAi && (
        <AiPanel
          onClose={() => setShowAi(false)}
          onGenerated={(yaml, title) => newTab(yaml, null, title)}
        />
      )}

      <div role="tablist" className="flex items-center gap-1 overflow-x-auto border-b border-edge bg-coal px-2 pt-1">
        {ws.tabs.map((t) => {
          const active = t.id === ws.activeId;
          return (
            <div
              key={t.id}
              role="tab"
              aria-selected={active}
              className={`group flex items-center gap-1.5 rounded-t-lg border-b-2 px-3 py-2 text-sm transition-colors ${
                active ? 'border-ember bg-ink text-white' : 'border-transparent text-smoke hover:bg-panel/60 hover:text-ash'
              }`}
            >
              <button className="flex items-center gap-1.5" onClick={() => setWs(selectTab(ws, t.id))}>
                {t.dirty && <span className="h-1.5 w-1.5 rounded-full bg-warn" aria-label="unsaved changes" />}
                {t.title}
              </button>
              <button
                aria-label={`close ${t.title}`}
                className="grid h-4 w-4 place-items-center rounded text-mist opacity-0 transition group-hover:opacity-100 hover:bg-edge hover:text-flare"
                onClick={() => setWs(closeTab(ws, t.id))}
              >
                <X />
              </button>
            </div>
          );
        })}
      </div>

      <div className="relative flex-1 overflow-hidden">
        {ws.tabs.map((t) => (
          <div key={t.id} className="absolute inset-0" hidden={t.id !== ws.activeId}>
            <Editor
              seedContent={seeds[t.id]?.content ?? STARTER}
              seedPath={seeds[t.id]?.path ?? null}
              onState={(s) => report(t.id, s)}
            />
          </div>
        ))}
        {ws.tabs.length === 0 && (
          <div className="grid h-full place-items-center text-smoke">
            <p>No open plans. Click <strong className="text-ash">New</strong> to start.</p>
          </div>
        )}
      </div>

      <footer className="flex items-center gap-2 border-t border-edge bg-coal px-4 py-1 text-xs text-mist">
        <span className={`h-1.5 w-1.5 rounded-full ${engineProblem ? 'bg-blood' : 'bg-ok'}`} />
        <span className="font-mono">{version || 'loadr —'}</span>
      </footer>
    </div>
  );
}
