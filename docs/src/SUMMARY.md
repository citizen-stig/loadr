# Summary

[Introduction](introduction.md)

# Getting started

- [Installation](getting-started/installation.md)
- [Your first test](getting-started/first-test.md)
- [The CLI](getting-started/cli.md)

# Writing tests (YAML reference)

- [Test definition overview](yaml/overview.md)
- [Scenarios & executors](yaml/scenarios-executors.md)
- [Requests](yaml/requests.md)
- [Flow control (loops, branches)](yaml/flow-control.md)
- [Extraction & correlation](yaml/extraction.md)
- [Assertions & checks](yaml/assertions-checks.md)
- [Thresholds](yaml/thresholds.md)
- [Data parameterization](yaml/data.md)
- [Feeders & throttling](yaml/feeders.md)
- [Variables, secrets & interpolation](yaml/variables.md)
- [Think time & pacing](yaml/timers.md)
- [Outputs](yaml/outputs.md)
- [Environments](yaml/environments.md)

# JavaScript

- [Embedded JavaScript overview](js/overview.md)
- [Lifecycle hooks](js/lifecycle.md)
- [JS API reference](js/api.md)

# Protocols

- [HTTP](protocols/http.md)
- [WebSocket](protocols/websocket.md)
- [Server-Sent Events](protocols/sse.md)
- [gRPC](protocols/grpc.md)
- [GraphQL](protocols/graphql.md)
- [Redis](protocols/redis.md)
- [Browser](protocols/browser.md)
- [TCP & UDP](protocols/sockets.md)

# Distributed testing

- [Overview](distributed/overview.md)
- [Controller & agents](distributed/controller-agents.md)
- [Metric aggregation](distributed/metrics-merging.md)

# Web UI

- [The management UI](webui.md)

# Plugins

- [Plugin system overview](plugins/overview.md)
- [WASM plugins](plugins/wasm.md)
- [Native plugins](plugins/native.md)
- [Developing a plugin](plugins/developing.md)

# Migration

- [Migrating from k6](migration/from-k6.md)
- [Migrating from JMeter](migration/from-jmeter.md)

# Reference

- [Built-in metrics](reference/metrics.md)
- [Exit codes](reference/exit-codes.md)
- [JSON Schema & editor setup](reference/json-schema.md)

# About

- [Credits & influences](credits.md)

# Architecture

- [Architecture overview](adr/architecture.md)
- [ADR-001: JavaScript runtime](adr/001-js-runtime.md)
- [ADR-002: Plugin system](adr/002-plugins.md)
- [ADR-003: Coordination protocol](adr/003-coordination.md)
- [ADR-004: HTTP stack](adr/004-http-stack.md)
- [ADR-005: Metrics engine](adr/005-metrics.md)
- [ADR-006: Executor model](adr/006-executors.md)
