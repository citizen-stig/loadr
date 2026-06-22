import { useEffect, useState } from 'react';

import { defaultModel, getProvider, PROVIDERS } from '../shared/providers';
import { Button, Field, IconButton, Segmented, Select, Textarea, TextInput } from './ui/controls';
import { FolderOpen, Key, Sparkles, X } from './ui/icons';

type Mode = 'prompt' | 'repo';

// "Generate with AI": describe a test in natural language, or point loadr at a
// repository, and get a validated plan in a new tab. Works with any of the big
// providers (Anthropic / OpenAI / Google / xAI) — pick one and set its key
// (stored OS-encrypted in main; never exposed to the renderer).
export function AiPanel({ onClose, onGenerated }: { onClose: () => void; onGenerated: (yaml: string, title: string) => void }) {
  const [provider, setProvider] = useState(() => localStorage.getItem('loadr.ai.provider') || 'anthropic');
  const prov = getProvider(provider);
  const [model, setModel] = useState(() => localStorage.getItem(`loadr.ai.model.${provider}`) || defaultModel(provider));
  const [mode, setMode] = useState<Mode>('prompt');
  const [prompt, setPrompt] = useState('');
  const [source, setSource] = useState('');
  const [hasKey, setHasKey] = useState<boolean | null>(null);
  const [keyInput, setKeyInput] = useState('');
  const [editingKey, setEditingKey] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);

  // Re-check the key whenever the provider changes (keys are per-provider).
  useEffect(() => {
    setHasKey(null);
    setEditingKey(false);
    setKeyInput('');
    window.loadr.ai.hasKey(provider).then(setHasKey).catch(() => setHasKey(false));
    localStorage.setItem('loadr.ai.provider', provider);
  }, [provider]);
  useEffect(() => { localStorage.setItem(`loadr.ai.model.${provider}`, model); }, [provider, model]);

  function changeProvider(id: string) {
    setProvider(id);
    setModel(localStorage.getItem(`loadr.ai.model.${id}`) || defaultModel(id));
  }

  async function saveKey() {
    if (!keyInput.trim()) return;
    await window.loadr.ai.setKey(provider, keyInput.trim());
    setKeyInput('');
    setEditingKey(false);
    setHasKey(true);
  }

  async function browse() {
    const dir = await window.loadr.ai.browseRepo();
    if (dir) setSource(dir);
  }

  const canGenerate = hasKey && !busy && (mode === 'prompt' ? prompt.trim().length > 0 : source.trim().length > 0);

  async function generate() {
    setBusy(true);
    setError(null);
    setNote(null);
    try {
      const r = await window.loadr.ai.generate({ provider, model, mode, prompt: prompt.trim(), source: source.trim() });
      const title = (r.yaml.match(/^name:\s*(.+)$/m)?.[1]?.trim() || 'ai plan').slice(0, 40);
      onGenerated(r.yaml, title);
      if (r.valid) {
        onClose();
      } else {
        const n = r.diagnostics.filter((d) => d.severity === 'error').length;
        setNote(`Opened in a new tab, but it still has ${n} validation error(s) after a repair pass — review the highlighted fields.`);
      }
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="absolute inset-0 z-50 flex items-center justify-center bg-black/70 p-6 backdrop-blur-sm" role="dialog" aria-label="Generate with AI">
      <div className="flex max-h-[88vh] w-[46rem] flex-col rounded-2xl border border-edge bg-panel shadow-2xl shadow-black/60">
        <div className="flex items-center justify-between border-b border-edge px-4 py-3">
          <h2 className="flex items-center gap-2 font-bold text-white"><span className="text-flare"><Sparkles /></span>Generate with AI</h2>
          <div className="flex items-center gap-1">
            {hasKey && !editingKey && (
              <button onClick={() => setEditingKey(true)} className="flex items-center gap-1 rounded-md px-2 py-1 text-xs text-mist hover:text-ash" title="Change API key">
                <Key /> key set
              </button>
            )}
            <IconButton icon={X} label="close" onClick={onClose} />
          </div>
        </div>

        <div className="flex-1 space-y-4 overflow-y-auto p-4">
          <div className="grid grid-cols-2 gap-3">
            <Field label="Provider">
              <Select value={provider} onChange={(e) => changeProvider(e.target.value)}>
                {PROVIDERS.map((p) => <option key={p.id} value={p.id}>{p.label}</option>)}
              </Select>
            </Field>
            <Field label="Model" hint="type any model the provider supports">
              <TextInput list="ai-models" value={model} onChange={(e) => setModel(e.target.value)} />
            </Field>
            <datalist id="ai-models">{prov.models.map((m) => <option key={m} value={m} />)}</datalist>
          </div>

          {hasKey === false || editingKey ? (
            <div className="rounded-xl border border-ember/40 bg-ember/5 p-3">
              <Field label={`${prov.label} API key`} hint={`stored encrypted in your OS keychain · get one at ${prov.keysUrl}`}>
                <div className="flex gap-2">
                  <TextInput type="password" value={keyInput} placeholder={prov.keyHint} onChange={(e) => setKeyInput(e.target.value)} />
                  <Button variant="primary" onClick={saveKey} disabled={!keyInput.trim()}>Save</Button>
                  {editingKey && <Button onClick={() => setEditingKey(false)}>Cancel</Button>}
                </div>
              </Field>
            </div>
          ) : null}

          <Segmented
            ariaLabel="generation source"
            value={mode}
            onChange={setMode}
            options={[{ value: 'prompt', label: 'Describe', icon: Sparkles }, { value: 'repo', label: 'From repository', icon: FolderOpen }]}
          />

          {mode === 'prompt' ? (
            <Field label="Describe the load test" hint="e.g. “100 VUs for 2 minutes against POST /checkout with a JSON body, assert 200 and p95 < 400ms”">
              <Textarea rows={5} value={prompt} placeholder="Ramp to 200 users over 1m hitting GET /api/products and /api/products/${id}, then hold 3m…" onChange={(e) => setPrompt(e.target.value)} />
            </Field>
          ) : (
            <div className="space-y-3">
              <Field label="Repository" hint="a local folder or an https git URL — loadr reads its OpenAPI spec / routes to build the test">
                <div className="flex gap-2">
                  <TextInput value={source} placeholder="/path/to/repo  or  https://github.com/org/api.git" onChange={(e) => setSource(e.target.value)} />
                  <Button icon={FolderOpen} onClick={browse}>Browse…</Button>
                </div>
              </Field>
              <Field label="Focus (optional)" hint="anything to steer the test — endpoints to emphasise, load profile, auth">
                <TextInput value={prompt} placeholder="emphasise the checkout flow; ramp to 300 VUs" onChange={(e) => setPrompt(e.target.value)} />
              </Field>
            </div>
          )}

          {error && <p className="rounded-lg border border-blood/40 bg-blood/10 px-2.5 py-1.5 text-xs text-flare">✗ {error}</p>}
          {note && <p className="rounded-lg border border-warn/40 bg-warn/10 px-2.5 py-1.5 text-xs text-warn">{note}</p>}
        </div>

        <div className="flex items-center justify-between gap-2 border-t border-edge px-4 py-3">
          <span className="text-xs text-mist">{busy ? 'Generating & validating against the loadr CLI…' : 'Output is validated and opens in a new tab.'}</span>
          <Button variant="primary" icon={Sparkles} onClick={generate} disabled={!canGenerate}>
            {busy ? 'Generating…' : 'Generate plan'}
          </Button>
        </div>
      </div>
    </div>
  );
}
