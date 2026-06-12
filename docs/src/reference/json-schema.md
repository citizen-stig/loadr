# JSON Schema & editor setup

loadr's YAML format ships as a JSON Schema generated from the same types the
parser uses — autocomplete and inline validation can never drift from
reality.

```bash
loadr schema > loadr.schema.json
```

## VS Code (YAML extension)

```jsonc
// .vscode/settings.json
{
  "yaml.schemas": {
    "./loadr.schema.json": ["**/loadtests/**/*.yaml", "**/*.loadr.yaml"]
  }
}
```

Or per file:

```yaml
# yaml-language-server: $schema=./loadr.schema.json
name: my-test
```

## JetBrains IDEs

Settings → Languages & Frameworks → Schemas and DTDs → JSON Schema Mappings →
add `loadr.schema.json` with your test file pattern.

## Neovim

```lua
require('lspconfig').yamlls.setup {
  settings = { yaml = { schemas = { ["./loadr.schema.json"] = "loadtests/**/*.yaml" } } }
}
```

## CI validation without an editor

```bash
loadr validate --format json loadtests/*.yaml
```

gives you the same diagnostics (path, line, column, message, suggestion) as
machine-readable JSON.
