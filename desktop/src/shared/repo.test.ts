import { describe, expect, it } from 'vitest';

import { extractEndpoints, summarizeRepo, type RepoFile } from './repo';

describe('extractEndpoints', () => {
  it('finds routes across frameworks', () => {
    const files: RepoFile[] = [
      { path: 'server.js', content: `app.get('/users', h); router.post("/login", h);` },
      { path: 'api.py', content: `@app.get("/items")\ndef items(): ...\n@app.route("/health")\ndef h(): ...` },
      { path: 'main.go', content: `mux.HandleFunc("/metrics", h)\nr.Post("/orders", h)` },
      { path: 'Ctl.java', content: `@GetMapping("/products")` },
    ];
    const eps = extractEndpoints(files);
    const keys = eps.map((e) => `${e.method} ${e.path}`);
    expect(keys).toContain('GET /users');
    expect(keys).toContain('POST /login');
    expect(keys).toContain('GET /items');
    expect(keys).toContain('GET /health');
    expect(keys).toContain('GET /metrics');
    expect(keys).toContain('POST /orders');
    expect(keys).toContain('GET /products');
  });

  it('dedupes and ignores non-paths', () => {
    const eps = extractEndpoints([{ path: 'a.js', content: `app.get('/x',h); app.get('/x',h); app.get('notapath',h)` }]);
    expect(eps).toEqual([{ method: 'GET', path: '/x' }]);
  });
});

describe('summarizeRepo', () => {
  it('prefers an OpenAPI spec and lists manifests/readme', () => {
    const files: RepoFile[] = [
      { path: 'package.json', content: '{"name":"svc"}' },
      { path: 'README.md', content: 'My service' },
      { path: 'openapi.yaml', content: 'openapi: 3.0.0\npaths:\n  /a: {}\n  /b: {}' },
    ];
    const ctx = summarizeRepo(files, 'svc');
    expect(ctx.openapi).toContain('paths:');
    expect(ctx.manifests).toContain('package.json');
    expect(ctx.readme).toBe('My service');
    expect(ctx.summary).toContain('OpenAPI/Swagger spec');
  });

  it('falls back to source-extracted endpoints when no spec', () => {
    const ctx = summarizeRepo([{ path: 'app.js', content: `app.get('/ping', h)` }]);
    expect(ctx.openapi).toBeNull();
    expect(ctx.summary).toContain('GET /ping');
  });
});
