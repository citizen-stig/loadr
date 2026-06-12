# GraphQL

GraphQL rides on the HTTP client (`protocol: graphql`): loadr builds the
standard `{query, variables, operationName}` POST envelope, then understands
GraphQL's error semantics on top of HTTP's.

```yaml
- request:
    name: search
    url: /graphql
    protocol: graphql
    graphql:
      query: |
        query Search($term: String!) {
          products(search: $term) { edges { node { id name } } totalCount }
        }
      variables: { term: "widget" }       # string leaves interpolate ${...}
      operation_name: Search
    extract:
      - { type: jsonpath, name: first_id, expression: "$.data.products.edges[0].node.id" }
    checks:
      - { type: jsonpath, name: no errors, expression: "$.errors", exists: false }
```

## Failure semantics

A GraphQL response is marked **failed** when:

- the HTTP layer failed (transport error or status ≥ 400), or
- the body has a non-empty `errors` array **and no `data`** (total failure).

Partial errors (`errors` alongside `data`) do *not* fail the request — assert
on them explicitly if they matter:

```yaml
assert:
  - { type: jsonpath, expression: "$.errors", exists: false }
```

## Metrics

GraphQL requests emit the full `http_*` family **plus** `graphql_reqs` and
`graphql_req_duration`, so you can threshold GraphQL separately:

```yaml
thresholds:
  graphql_req_duration: [ "p(95)<400" ]
```

`extras.graphql_errors` carries the error count for `js` conditions.
