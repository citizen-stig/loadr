import { useEffect, useState } from 'react';

import type { InstalledPlugin } from '../shared/plugins';
import { Badge, Button, IconButton, TextInput } from './ui/controls';
import { FolderOpen, Puzzle, Trash, X } from './ui/icons';

// M5: manage protocol plugins via `loadr plugin` — list, install (by index name,
// directory or URL) and remove. Shown as a modal over the workspace.
export function PluginsPanel({ onClose }: { onClose: () => void }) {
  const [plugins, setPlugins] = useState<InstalledPlugin[]>([]);
  const [spec, setSpec] = useState('');
  const [allowUntrusted, setAllowUntrusted] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  const reload = () => window.loadr.pluginList().then(setPlugins).catch(() => {});
  useEffect(() => { reload(); }, []);

  async function install() {
    if (!spec.trim()) return;
    setBusy(true);
    setMessage(null);
    try {
      setMessage(await window.loadr.pluginInstall(spec.trim(), allowUntrusted));
      setSpec('');
      await reload();
    } catch (e) {
      setMessage(`✗ ${(e as Error).message}`);
    } finally {
      setBusy(false);
    }
  }

  async function browse() {
    const dir = await window.loadr.pluginBrowseDir();
    if (dir) setSpec(dir);
  }

  async function remove(name: string) {
    setBusy(true);
    try {
      await window.loadr.pluginRemove(name);
      await reload();
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="absolute inset-0 z-50 flex items-center justify-center bg-black/70 p-6 backdrop-blur-sm" role="dialog" aria-label="Plugins">
      <div className="flex max-h-[80vh] w-[42rem] flex-col rounded-2xl border border-edge bg-panel shadow-2xl shadow-black/60">
        <div className="flex items-center justify-between border-b border-edge px-4 py-3">
          <h2 className="flex items-center gap-2 font-bold text-white"><span className="text-flare"><Puzzle /></span>Plugins</h2>
          <IconButton icon={X} label="close plugins" onClick={onClose} />
        </div>

        <div className="flex-1 overflow-y-auto px-4 py-3">
          {plugins.length === 0 ? (
            <p className="rounded-lg border border-dashed border-edge px-3 py-6 text-center text-sm text-mist">No plugins installed.</p>
          ) : (
            <table className="w-full text-sm">
              <thead><tr className="text-left text-[11px] uppercase tracking-wide text-mist"><th className="pb-1 font-semibold">name</th><th className="pb-1 font-semibold">kind</th><th className="pb-1 font-semibold">version</th><th className="pb-1 font-semibold">state</th><th></th></tr></thead>
              <tbody>
                {plugins.map((p) => (
                  <tr key={p.name} className="border-t border-edge/60">
                    <td className="py-1.5 font-mono text-ash">{p.name}</td>
                    <td className="text-smoke">{p.kind}</td>
                    <td className="font-mono text-smoke">{p.version}</td>
                    <td><Badge tone={p.state === 'enabled' ? 'ok' : 'neutral'}>{p.state}</Badge></td>
                    <td className="text-right"><IconButton icon={Trash} tone="danger" label={`remove ${p.name}`} disabled={busy} onClick={() => remove(p.name)} /></td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>

        <div className="border-t border-edge px-4 py-3">
          <label className="text-[11px] font-semibold uppercase tracking-wide text-smoke">Install (index name, directory or URL)</label>
          <div className="mt-1.5 flex gap-2">
            <TextInput
              value={spec}
              placeholder="mongo  ·  /path/to/dist  ·  github:owner/repo"
              onChange={(e) => setSpec(e.target.value)}
              aria-label="plugin spec"
            />
            <Button icon={FolderOpen} onClick={browse}>Browse…</Button>
            <Button variant="primary" onClick={install} disabled={busy || !spec.trim()}>Install</Button>
          </div>
          <label className="mt-2 flex items-center gap-2 text-xs text-mist">
            <input type="checkbox" checked={allowUntrusted} onChange={(e) => setAllowUntrusted(e.target.checked)} className="accent-ember" />
            allow untrusted (non-index sources)
          </label>
          {message && <pre className="mt-2 max-h-24 overflow-y-auto whitespace-pre-wrap rounded-lg border border-edge bg-coal p-2 text-xs text-smoke">{message}</pre>}
        </div>
      </div>
    </div>
  );
}
