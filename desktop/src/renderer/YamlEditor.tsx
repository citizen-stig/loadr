// Self-hosted Monaco YAML editor (no CDN — CSP is `default-src 'self'`). The
// component is intentionally thin: it's a controlled view over the doc's YAML
// text. The two-way sync logic lives in usePlanDoc; Monaco rendering itself is
// verified by the Playwright-for-Electron e2e (M6), since it needs a real DOM.

import Editor, { loader } from '@monaco-editor/react';
import * as monaco from 'monaco-editor';
import editorWorker from 'monaco-editor/esm/vs/editor/editor.worker?worker';

// Route Monaco to the locally-bundled build + worker (self-hosted).
loader.config({ monaco });
self.MonacoEnvironment = {
  getWorker: () => new editorWorker(),
};

export function YamlEditor({
  value,
  onChange,
}: {
  value: string;
  onChange: (next: string) => void;
}) {
  return (
    <Editor
      height="100%"
      defaultLanguage="yaml"
      theme="vs-dark"
      value={value}
      onChange={(v) => onChange(v ?? '')}
      options={{
        minimap: { enabled: false },
        fontSize: 13,
        tabSize: 2,
        scrollBeyondLastLine: false,
        automaticLayout: true,
      }}
    />
  );
}
