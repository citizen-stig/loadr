# Writing a plugin in another language (C ABI)

loadr's [native plugins](native.md) normally use
[`abi_stable`](https://docs.rs/abi_stable), whose compile-time layout
handshake is **Rust-to-Rust only** — no other language can reproduce it. To
let you write a **protocol** plugin in C, Go, Zig, or anything that can emit a
C shared library, loadr also accepts plugins built against a small, frozen
**plain C ABI**: pointers, lengths, and a plugin-owned allocator. No
`abi_stable`, no Rust types cross the boundary.

When a native library is loaded, loadr **probes for the C entry symbol**
(`loadr_plugin_abi_version`). If present it is loaded as a C-ABI plugin;
otherwise it falls back to the abi_stable path. Both kinds route through the
same engine machinery, so scheme routing, metrics, and `plugin.toml` work
identically.

> **Scope.** The C ABI currently covers `protocol` plugins only. Outputs and
> services remain Rust/abi_stable (they have richer, stateful lifecycles).

## The C symbol contract (ABI version 1)

A C-ABI plugin is a shared library that exports exactly these four
`extern "C"` symbols:

```c
#include <stddef.h>
#include <stdint.h>

// The C-ABI version this plugin targets. The host refuses to load a plugin
// whose version it does not understand (current host version: 1).
uint32_t loadr_plugin_abi_version(void);

// PluginInfo as UTF-8 JSON; *out_len receives the byte length.
// Buffer is plugin-owned: the host copies it, then calls loadr_plugin_free.
uint8_t *loadr_plugin_info(size_t *out_len);

// Execute one request. `req`/`req_len` is a UTF-8 JSON FfiRequest.
// Returns a UTF-8 JSON FfiResponse of length *out_len, plugin-owned
// (freed via loadr_plugin_free).
uint8_t *loadr_plugin_execute(const uint8_t *req, size_t req_len, size_t *out_len);

// Free a buffer previously returned by info()/execute(), with the exact
// ptr/len the plugin returned.
void loadr_plugin_free(uint8_t *ptr, size_t len);
```

### Allocator rule

Every buffer the plugin returns is **plugin-owned**. The host copies the bytes
it needs and then hands the buffer back to `loadr_plugin_free(ptr, len)` with
the exact pointer and length the plugin returned. This keeps allocation and
deallocation on the same side of the boundary — the host never frees plugin
memory with its own allocator. A null return with `*out_len == 0` is treated
as an empty buffer and is **not** passed to `free`.

### Threading rule

The host calls `loadr_plugin_execute` **concurrently from many worker
threads** (one virtual user per thread, all sharing the one loaded library).
Your `execute` **must be thread-safe** — exactly the contract the Rust
`FfiProtocol: Send + Sync` bound expresses. `info` and `abi_version` are called
once, on the loading thread, before any `execute`.

### No unwinding across the boundary

`execute` must not let an exception / panic / `longjmp` cross the FFI boundary
(undefined behaviour). Report failures in the response `error` field instead.

### ABI versioning

`loadr_plugin_abi_version` returns the C-ABI version the plugin was written
against. The host compares it to its own `LOADR_C_ABI_VERSION` (currently `1`)
and refuses to load a mismatch with a clear error. This version is **separate**
from the abi_stable surface version; the two evolve independently. It is bumped
only on an incompatible change to the four symbols above. (Adding a field to
the request/response JSON is *not* a break — see below.)

## The JSON request / response shapes

Payloads cross as JSON, identical to the abi_stable path
(`loadr_plugin_api::native::FfiRequest` / `FfiResponse`). Adding a field is
forward-compatible, never an ABI break.

**Request** (`loadr_plugin_execute` input):

```jsonc
{
  "name": "echo something",      // request name from the YAML flow
  "method": "SEND",
  "url": "cecho://host/path",
  "headers": [["x-test", "1"]],
  "body_b64": "cGluZw==",        // request body, base64
  "timeout_ms": 5000,
  "options": { ... },             // the request's `plugin:` block (may be absent)
  "config": { ... }               // manifest [config] + per-run overrides
}
```

**Response** (`loadr_plugin_execute` output):

```jsonc
{
  "status": 200,                  // i64; your protocol's status code
  "status_text": "OK",
  "headers": [["x-cecho", "1"]],
  "body_b64": "cGluZw==",        // response body, base64
  "duration_ms": 1.5,             // request latency you measured
  "error": null,                  // a string fails the request
  "extras": { "echoed_by": "c-echo" }  // free-form; surfaces in metrics/checks
}
```

All fields except `status`/`body_b64` are optional and default sensibly.

## `plugin.toml`

Package the library with a manifest, same as any native plugin. You may add an
optional `abi = "c"` hint, but it is not required — the host auto-detects:

```toml
[plugin]
name = "cecho"
version = "0.1.0"
kind = "protocol"
type = "native"
abi = "c"                 # optional hint; "native" forces abi_stable
entry = "libloadr_plugin_cecho.so"
description = "Echo protocol plugin written in C (C-ABI)"
schemes = ["cecho"]       # URL scheme(s) this plugin serves
```

## Worked example: `c-echo`

A complete, dependency-free C plugin ships in
[`examples/plugins/c-echo/`](https://github.com/levantar-ai/loadr/tree/main/examples/plugins/c-echo).
It serves the `cecho://` scheme and echoes each request body back with status
200.

### Build it

```bash
cd examples/plugins/c-echo
make                 # -> libloadr_plugin_cecho.so
```

Platform notes for the shared library:

| Platform | Command | Artifact |
|---|---|---|
| Linux   | `cc -O2 -fPIC -shared -o libloadr_plugin_cecho.so cecho.c` | `.so` |
| macOS   | `cc -O2 -fPIC -dynamiclib -o libloadr_plugin_cecho.dylib cecho.c` | `.dylib` |
| Windows | `cl /LD /Fe:loadr_plugin_cecho.dll cecho.c` | `.dll` |

Set `entry` in `plugin.toml` to match the artifact name for your platform.

### Run it

Reference the built artifact straight from a test plan:

```yaml
name: c-echo-smoke
plugins:
  - name: cecho
    path: examples/plugins/c-echo/libloadr_plugin_cecho.so

scenarios:
  echo:
    executor: shared-iterations
    vus: 2
    iterations: 4
    flow:
      - request:
          name: echo something
          url: cecho://localhost/whatever
          method: SEND
          body: "ping-from-loadr"
          assert:
            - { type: status, equals: 200 }
            - { type: body_contains, value: "ping-from-loadr" }
```

```console
$ loadr run c-echo-smoke.yaml
  c-echo-smoke — 1 scenario(s)
  cecho_reqs....................: 4
  http_req_failed...............: 0.00% — ✓ 0 ✗ 4
```

The `cecho://` scheme routed to the plugin, every request echoed its body, and
both assertions passed. The metric family (`cecho_*`) is derived from the
plugin's `name`, exactly as for Rust native plugins.

### Implementing it

The interesting parts of `cecho.c`:

```c
#define LOADR_C_ABI_VERSION 1u

uint32_t loadr_plugin_abi_version(void) { return LOADR_C_ABI_VERSION; }

void loadr_plugin_free(uint8_t *ptr, size_t len) { (void)len; free(ptr); }

uint8_t *loadr_plugin_info(size_t *out_len) {
    // malloc'd JSON: name/version/kind="protocol"/description/schemes
    return dup_bytes("{\"name\":\"cecho\", ... ,\"schemes\":[\"cecho\"]}", out_len);
}

uint8_t *loadr_plugin_execute(const uint8_t *req, size_t req_len, size_t *out_len) {
    // 1. read body_b64 / method out of the request JSON
    // 2. build a malloc'd FfiResponse JSON that echoes the body
    // 3. write its length to *out_len and return it
}
```

`c-echo` does minimal hand-rolled JSON scanning to stay dependency-free; a real
plugin would link a JSON library (e.g. cJSON, or use Go's `encoding/json`).

## Other languages

Any toolchain that produces a C shared library exporting the four symbols
works. For example, a **Go** plugin built with `go build -buildmode=c-shared`
uses `//export` directives and `C.malloc`/`C.free` for the returned buffers
(so the host's `loadr_plugin_free` matches Go's C allocator), keeping the
allocator contract intact.

## Safety

Like all native plugins, C-ABI plugins run **in-process with full privileges**
— treat them as trusted code. The host validates the ABI version on load and
copies every buffer immediately, but it cannot sandbox native code. Prefer
[WASM plugins](wasm.md) for anything that does not need native capability.
