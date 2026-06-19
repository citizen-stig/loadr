import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    // Default to node (the round-trip core uses fs/child_process). Component
    // tests opt into jsdom per-file via `// @vitest-environment jsdom`.
    environment: 'node',
    // globals: true makes afterEach available so @testing-library/react's
    // automatic per-test cleanup runs (otherwise mounted trees leak between
    // tests and queries become ambiguous).
    globals: true,
    include: ['src/**/*.test.ts', 'src/**/*.test.tsx'],
  },
});
