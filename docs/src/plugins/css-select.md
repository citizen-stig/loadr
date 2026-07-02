# css-select plugin

> **Status:** planned — this page documents the intended contract; the plugin is
> not yet shipped in the plugin index.

`loadr-plugin-css-select` adds a **CSS-selector extractor**. It is a native
`extractor` plugin: given an HTML response body, it parses the document and
applies a CSS selector, returning the **text** of the first match — or the value
of a named **attribute**. Parsing is pure Rust (the [`scraper`] / [`selectors`]
crates), so there is no headless browser and no C dependency; the whole response
is parsed once per extraction and the first matching node wins.

It exists to pull values out of *rendered pages* rather than JSON APIs: a CSRF
token in a hidden `<input>`, a nonce in a `<meta>` tag, a signed URL in an
`<a href>`. The extracted string flows into a `${var}` you can reuse in later
requests, exactly like the built-in extractors.

The plugin implements the native `PluginExtractor` trait; the contract is
documented in [Developing a plugin](developing.md).

[`scraper`]: https://docs.rs/scraper
[`selectors`]: https://docs.rs/selectors

## Build and install

```bash
cargo build -p loadr-plugin-css-select --release

# `loadr plugin install` copies a directory that holds plugin.toml next to the
# artifact named by its `entry`. Stage the built cdylib beside the manifest:
mkdir -p dist
cp plugins/loadr-plugin-css-select/plugin.toml dist/
cp target/release/libloadr_plugin_css_select.so dist/   # .dylib on macOS, .dll on Windows
loadr plugin install dist
loadr plugin info css-select
```

Installing copies `plugin.toml` and the artifact into
`~/.loadr/plugins/css-select/`. The manifest declares it as a native extractor:

```toml
[plugin]
name = "css-select"
kind = "extractor"
type = "native"
entry = "libloadr_plugin_css_select.so"
description = "Extract text or an attribute from HTML via a CSS selector"
```

## Use it in a test

List the plugin under `plugins:` with its `config`, then reference it from an
`extract:` step by `type: plugin`. Plugin extractors are addressed by plugin
**name**; the `config` from the `plugins:` entry is passed to every call.

```yaml
plugins:
  - name: css-select
    config: { selector: "input[name=csrf]", attr: value }

scenarios:
  checkout:
    executor: constant-vus
    vus: 3
    duration: 1m
    flow:
      - request:
          name: form page
          url: /checkout/start
          extract:
            # Pull the hidden CSRF token's `value` attribute out of the rendered form
            - { type: plugin, name: csrf, plugin: css-select }
      - request:
          name: submit
          method: POST
          url: /checkout/submit
          body:
            form:
              csrf: ${csrf}
          assert:
            - { type: status, equals: 200 }
```

The `config` is JSON-shaped and handed to the plugin as-is, e.g.
`{"selector":"input[name=csrf]","attr":"value"}`.

## Config reference

| Key        | Type   | Required | Meaning |
|------------|--------|----------|---------|
| `selector` | string | yes      | A CSS selector applied to the parsed document. The **first** matching element is used. |
| `attr`     | string | no       | Name of the attribute to return (e.g. `value`, `href`, `content`). When omitted, the element's **text content** is returned instead. |

If the selector matches nothing — or `attr` is set but that attribute is absent
on the matched element — the extraction yields **no value** (the same as any
other extractor that fails to match). An invalid selector is a configuration
error surfaced when the plugin is first called.

## Metrics

**n/a.** Extractor plugins run inside an existing request's lifecycle and do not
emit their own metric family; the surrounding HTTP request is still measured by
the core `http_*` metrics.

## Notes

- **Relationship to the built-in `css` extractor.** loadr core already ships a
  first-class CSS extractor — `{ type: css, name: csrf, expression:
  "input[name=csrf]", attribute: value }` (see `examples/06-correlation.yaml`).
  This plugin is the same idea packaged as an installable extractor: reach for
  it when you want the selector engine versioned and shipped independently of
  the core binary, or as a worked reference for the `PluginExtractor` trait.
- **First match wins.** Like the built-in extractor, only the first element the
  selector matches is considered; narrow the selector if a page has several
  candidates.
- **HTML, not XML/JSON.** The body is parsed as HTML5. For JSON responses use
  `type: jsonpath`; for arbitrary text use `type: regex` or `type: boundary`.
- The plugin's `config` is fixed per `plugins:` entry. To extract with several
  different selectors in one plan, either list the plugin more than once under
  distinct names or fall back to the built-in `type: css` extractor, whose
  selector is specified per `extract:` step.
