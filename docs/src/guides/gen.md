# Generating from a contract (loadr gen)

`loadr convert` and `loadr record` start from *traffic*. `loadr gen` starts from
a *contract*: point it at an OpenAPI document and it emits a runnable scenario
with one request per operation, every parameter and body filled from
schema-derived example data.

## Quick start

```console
$ loadr gen openapi openapi.yaml -o plan.yaml
warning: [scenario `api`] generated 34 request(s); defaulted to constant-vus 1 VU for 60s — set real load
gen: wrote plan.yaml

$ loadr run plan.yaml
```

Every endpoint gets exercised with valid example data — no hand-writing.

## What it fills in

For each operation under `paths`:

- **Path params** (`/pets/{petId}`) → the `{param}` is replaced with a
  schema-derived example (`/pets/string`).
- **Query / header params** → the request's `params` / `headers`.
- **JSON request body** → an example built from `requestBody`'s schema, honouring
  `required`, `default`, `enum`, `example`, and `format` (dates, UUIDs, emails…).
- **Base URL** ← the spec's `servers[]` (choose with `--server N`, or override
  with `--base-url`).
- A **status assertion** from the operation's declared 2xx codes, so the plan is
  self-checking out of the box.

`$ref`s are resolved locally, and self-referential schemas terminate safely.

```yaml
scenarios:
  api:
    executor: constant-vus      # a safe default — set real load next
    vus: 1
    duration: 1m
    flow:
    - request:
        name: createPet
        method: POST
        url: /pets
        body:
          json: { name: string, tag: string }   # ← from the schema
        assert:
        - { type: status, one_of: [201] }        # ← from responses
```

## Options

| Flag | Meaning |
|------|---------|
| `-o, --output <path>` | Write the plan here (default: stdout) |
| `--base-url <url>` | Override the derived base URL |
| `--server <n>` | Index into the OpenAPI `servers[]` array |
| `--include <glob>` | Only operations whose operationId/path matches (repeatable) |
| `--exclude <glob>` | Drop matching operations (repeatable) |

`--include`/`--exclude` accept simple `*` globs (`get*`, `*Pet`, `*/pets/*`) and
keep the output tractable for large specs.

## Next steps

- Set a real `executor`, `vus`/`rate` and `duration`
  (see [Scenarios & executors](../yaml/scenarios-executors.md)).
- Add [thresholds](../yaml/thresholds.md) for a pass/fail gate.
- Review the example data — swap placeholders for realistic values or a
  [feeder](../yaml/feeders.md).
