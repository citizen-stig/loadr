# xpath plugin

> **Status:** planned — this plugin is not in the signed index yet. The shape
> below describes the intended extractor contract; treat it as a design note
> until it ships. loadr core already ships an equivalent **built-in**
> [`type: xpath` extractor](../yaml/extraction.md); this page covers the
> installable, plugin-packaged form of the same capability.

`loadr-plugin-xpath` is an **extractor plugin** (`kind = "extractor"`, role:
*Extractors*). It parses the response body as XML and evaluates an **XPath 1.0**
expression, returning the text of the first matching node. It is **pure Rust**,
built on the `sxd-xpath` / `roxmltree` stack — **no libxml2**, no C toolchain, no
system XML library to install. Once the plugin is installed it adds a new
`extract:` type keyed by its name, so a `{ "xpath": "//order/@id" }` config pulls
a value out of an XML response into a variable for later steps.

It implements the core [`PluginExtractor`](developing.md) trait: loadr hands it
the response and the plugin's `config` object, and it returns `Some(text)` for a
match or `None` for a miss — a failed extraction is surfaced as a miss with a
reason, never as an engine crash.

## Install

Once published, `xpath` will resolve from the signed
[plugin index](installing.md) by name — no build toolchain required:

```bash
loadr plugin install xpath
loadr plugin info xpath
```

This picks the artifact for your host target, checks it against the plugin ABI
your `loadr` build provides, verifies its sha256 and unpacks it into your
plugins directory (`~/.loadr/plugins/xpath/`, or `$LOADR_PLUGINS_DIR`).

The installed manifest declares a WASM extractor plugin — one portable artifact
runs on every platform:

```toml
[plugin]
name = "xpath"
version = "0.1.0"
kind = "extractor"
type = "wasm"
entry = "loadr_plugin_xpath.wasm"
description = "Extract the first XPath 1.0 match from an XML response body"
```

To run straight from a build tree instead, point the plan's `plugins:` entry at
the built artifact (`path: target/wasm32-wasip2/release/loadr_plugin_xpath.wasm`)
rather than resolving it by name.

## Use it in a test

List the plugin under `plugins:`, then reference it as an `extract:` type. The
`xpath:` key is the expression; `name:` is the variable the match is saved under,
available to every later step as `${name}` and to JS as `session.vars.name`.

```yaml
plugins:
  - name: xpath                    # or: { name: xpath, path: target/wasm32-wasip2/release/loadr_plugin_xpath.wasm }

scenarios:
  orders:
    executor: constant-vus
    vus: 10
    duration: 30s
    flow:
      - request:
          name: create order
          method: POST
          url: https://api.example.com/orders
          headers:
            Content-Type: "application/xml"
          body: |
            <order><item sku="A-100" qty="2"/></order>
          extract:
            # `type` is the plugin name; `xpath` is the plugin config.
            - { type: xpath, name: order_id, xpath: "//order/@id" }
          checks:
            - { type: status, equals: 201 }

      - request:
          name: fetch order
          url: https://api.example.com/orders/${order_id}
          checks:
            - { type: status, equals: 200 }
```

The `//order/@id` expression selects the `id` attribute of the first `<order>`
element; the plugin returns its text. Namespaced documents (SOAP, RSS, Atom) are
best matched with `local-name()`, e.g.
`//*[local-name()='ConversionRateResult']/text()`, which keeps the expression
namespace-agnostic. A complete XML/SOAP plan using XPath extraction lives in
[`examples/42-soap.yaml`](https://github.com/levantar-ai/loadr/blob/main/examples/42-soap.yaml).

## Config reference

The extractor is configured by the object on its `extract:` entry. Everything
except loadr's own `type`/`name` keys is passed to the plugin verbatim as its
`config`:

| Key       | Type   | Default        | Meaning |
|-----------|--------|----------------|---------|
| `type`    | string | *(required)*   | Must be `xpath` — the plugin name that routes this entry to the plugin. |
| `name`    | string | *(required)*   | Variable to save the match under (`${name}`, `session.vars.name`). |
| `xpath`   | string | *(required)*   | The XPath 1.0 expression to evaluate against the XML body. |
| `default` | string | *(none)*       | Value used when the expression matches nothing. **Without it, a no-match marks the request failed** (`http_req_failed`) and leaves the variable unset. |

The plugin `config` object itself is just `{ "xpath": "<expression>" }`; `name`
and `default` are handled by loadr's extraction machinery around the plugin.

### What a match returns

- **Node set** — the plugin returns the **text of the first node** in document
  order (an element's text content, or an attribute's value for an `@attr`
  expression).
- **No match / empty node set** — treated as a miss (`None`), so `default:`
  applies or the request is marked failed.
- **Malformed XML** — a parse failure is a miss with a reason, not a crash; the
  same `default:` / failure handling applies.

## Metrics

**n/a.** An extractor plugin emits no metric family of its own — it only pulls a
value out of a response that some other request already made. Its effect shows up
through the normal extraction path: a miss without a `default:` marks the request
failed and is reflected in `http_req_failed` and the `checks` rate, which you can
gate on in `thresholds:`:

```yaml
thresholds:
  http_req_failed: [ "rate<0.01" ]
  checks:          [ "rate>0.99" ]
```

## Notes

- **Pure Rust, no libxml2.** Parsing and evaluation run on the `sxd-xpath` /
  `roxmltree` stack, so there is no C library to install and the WASM artifact is
  fully sandboxed — the same engine core uses for the built-in `type: xpath`
  extractor.
- **XPath 1.0.** Full XPath 2.0/3.1 features (sequences, `matches()`, etc.) are
  not available; use axes, predicates and `local-name()` to select nodes.
- **First match only.** The plugin returns a single value — the first matching
  node's text. To capture every match as a JSON array, prefer the built-in
  extractors that support `index: all`.
- **Relationship to the built-in.** Core already resolves `type: xpath` without
  any plugin installed; this plugin exists to package the extractor as an
  independently versioned, installable artifact and to serve as the reference
  WASM extractor. If both are present, the installed plugin does not replace the
  built-in — reach for the plugin only when you need the standalone package.
- **Interpolation.** `${...}` interpolation works inside the `xpath:` string, so
  a per-VU or data-feed value can be spliced into the expression before it is
  evaluated.
</content>
</invoke>
