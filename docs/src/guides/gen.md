# Generating from a contract (loadr gen)

`loadr convert` and `loadr record` start from *traffic*. `loadr gen` starts from
a *contract*: point it at an OpenAPI document, a Postman collection, or a GraphQL schema and it emits a runnable scenario
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

## From a GraphQL schema

Point it at a GraphQL **introspection result** (the JSON from an introspection
query) and it builds one operation per `Query`/`Mutation` field:

```console
$ loadr gen graphql introspection.json --base-url https://api.example.com/graphql -o plan.yaml
```

Each field's arguments are lifted to GraphQL variables (seeded with example
values), and object return types are expanded into a selection set to a bounded
depth (cycle-guarded):

```yaml
graphql:
  query: |
    query product($id: ID!) {
      product(id: $id) { id name }
    }
  variables: { id: id }
  operation_name: product
```

Pass the endpoint with `--base-url`.

## From a Postman collection

```console
$ loadr gen postman collection.json -o plan.yaml
```

Folders become groups, requests become steps, and Postman `{{var}}` placeholders
become loadr `${var}` interpolation — set them via env or `--var` at run time.

## Fuzzing a contract (`--fuzz`)

A contract promises to *reject* bad input — not crash on it. `--fuzz` turns that
promise into a test. For every operation with a JSON body it appends variant
requests beside the valid one, each asserting the status is **2xx–4xx (never a
5xx)**:

```console
$ loadr gen openapi openapi.yaml --fuzz -o fuzz.yaml
$ loadr run fuzz.yaml     # a 5xx on any variant fails the run
```

Three variant families:

- **Structural** — drop a `required` key; swap a field to the wrong type.
- **Boundary** — values that violate the schema's bounds.
- **Adversarial** — the body replaced with a [`loadr payload`](payload.md) entry
  (`nested-json`, `long-string`, …), reusing the algorithmic-complexity catalog.
  Choose kinds with `--fuzz-payloads nested-json,billion-laughs`.

Each variant is named `... [fuzz: <what>]` and carries a `status` assertion
matching `^[234]..$`, so a crash (5xx) is a test failure.

## Next steps

- Set a real `executor`, `vus`/`rate` and `duration`
  (see [Scenarios & executors](../yaml/scenarios-executors.md)).
- Add [thresholds](../yaml/thresholds.md) for a pass/fail gate.
- Review the example data — swap placeholders for realistic values or a
  [feeder](../yaml/feeders.md).
