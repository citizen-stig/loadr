// Pure repository analysis for AI test generation. Given a set of files read
// from a target repo, extract the signals an LLM needs to write a realistic
// load test: an OpenAPI/Swagger spec (the strongest signal), HTTP endpoints
// declared in common web frameworks, dependency manifests, and a README
// excerpt — all bounded so the prompt stays small. Pure → headless-testable;
// the filesystem walk / git clone lives in main/repo.ts.

export interface RepoFile {
  path: string;
  content: string;
}

export interface Endpoint {
  method: string;
  path: string;
}

export interface RepoContext {
  name: string;
  openapi: string | null; // trimmed spec text, if found
  endpoints: Endpoint[];
  manifests: string[]; // e.g. ["package.json", "go.mod"]
  readme: string | null; // excerpt
  summary: string; // compact text block to feed the model
}

const MAX_OPENAPI = 16000;
const MAX_README = 1500;
const MAX_ENDPOINTS = 50;

// (method, path) extractors for the common web frameworks. Each returns matches
// over a file's content; `m[1]` = method (may be empty), `m[2]` = path.
const ROUTE_PATTERNS: { re: RegExp; methodFrom: 'g1' | 'fixed'; fixed?: string }[] = [
  // Express / Koa / Fastify / Hono (JS/TS): app.get('/x'), router.post("/y")
  { re: /\b(?:app|router|fastify|server|api|r)\.(get|post|put|patch|delete|all)\s*\(\s*['"`]([^'"`]+)['"`]/gi, methodFrom: 'g1' },
  // Flask / FastAPI decorators: @app.get("/x"), @router.post('/y')
  { re: /@(?:app|router)\.(get|post|put|patch|delete)\s*\(\s*['"]([^'"]+)['"]/gi, methodFrom: 'g1' },
  // Flask route: @app.route("/x")
  { re: /@(?:app|bp)\.route\s*\(\s*['"]([^'"]+)['"]/gi, methodFrom: 'fixed', fixed: 'GET' },
  // Go net/http + chi/gorilla: mux.HandleFunc("/x"), r.Get("/y")
  { re: /\.(HandleFunc|Handle|Get|Post|Put|Patch|Delete)\s*\(\s*"([^"]+)"/g, methodFrom: 'g1' },
  // Spring (Java/Kotlin): @GetMapping("/x"), @RequestMapping(value="/y")
  { re: /@(Get|Post|Put|Patch|Delete|Request)Mapping\s*\(\s*(?:value\s*=\s*)?["']([^"']+)["']/g, methodFrom: 'g1' },
  // Rails routes.rb: get "/x", post '/y'
  { re: /^\s*(get|post|put|patch|delete)\s+['"]([^'"]+)['"]/gim, methodFrom: 'g1' },
];

const OPENAPI_RE = /(?:"openapi"\s*:|openapi\s*:|"swagger"\s*:|swagger\s*:)/i;
const MANIFESTS = ['package.json', 'go.mod', 'Cargo.toml', 'requirements.txt', 'pyproject.toml', 'pom.xml', 'build.gradle', 'Gemfile'];

const HTTP_METHODS = new Set(['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS']);

function norm(method: string, p: string): Endpoint {
  // Handler-registration verbs (Go HandleFunc/Handle, Express `all`, Spring
  // `Request`) carry no specific method — treat them as GET.
  const m = (method || 'GET').toUpperCase();
  return { method: HTTP_METHODS.has(m) ? m : 'GET', path: p };
}

export function extractEndpoints(files: RepoFile[]): Endpoint[] {
  const seen = new Set<string>();
  const out: Endpoint[] = [];
  for (const f of files) {
    for (const { re, methodFrom, fixed } of ROUTE_PATTERNS) {
      re.lastIndex = 0;
      let m: RegExpExecArray | null;
      while ((m = re.exec(f.content)) !== null) {
        const path = methodFrom === 'fixed' ? m[1] : m[2];
        const method = methodFrom === 'fixed' ? (fixed ?? 'GET') : m[1];
        if (!path || !path.startsWith('/')) continue;
        const e = norm(method, path);
        const key = `${e.method} ${e.path}`;
        if (seen.has(key)) continue;
        seen.add(key);
        out.push(e);
        if (out.length >= MAX_ENDPOINTS) return out;
      }
    }
  }
  return out;
}

export function summarizeRepo(files: RepoFile[], name = 'target'): RepoContext {
  const base = (p: string) => p.split('/').pop() || p;

  const openapiFile = files.find(
    (f) => /(openapi|swagger)\.(ya?ml|json)$/i.test(f.path) || (OPENAPI_RE.test(f.content) && /paths/i.test(f.content)),
  );
  const openapi = openapiFile ? openapiFile.content.slice(0, MAX_OPENAPI) : null;

  const endpoints = extractEndpoints(files);

  const manifests = [...new Set(files.map((f) => base(f.path)).filter((b) => MANIFESTS.includes(b)))];

  const readmeFile = files.find((f) => /^readme(\.md)?$/i.test(base(f.path)));
  const readme = readmeFile ? readmeFile.content.slice(0, MAX_README) : null;

  const parts: string[] = [`Repository: ${name}`];
  if (manifests.length) parts.push(`Manifests detected: ${manifests.join(', ')}`);
  if (readme) parts.push(`README excerpt:\n${readme}`);
  if (openapi) {
    parts.push(`OpenAPI/Swagger spec (authoritative list of endpoints):\n${openapi}`);
  } else if (endpoints.length) {
    parts.push(`HTTP endpoints found in source:\n${endpoints.map((e) => `${e.method} ${e.path}`).join('\n')}`);
  } else {
    parts.push('No OpenAPI spec or HTTP routes were detected; infer a sensible test from the manifests/README.');
  }

  return { name, openapi, endpoints, manifests, readme, summary: parts.join('\n\n') };
}
