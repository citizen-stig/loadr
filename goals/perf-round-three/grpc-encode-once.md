# Goal - encode rendered gRPC messages once

On the templated-message path (any message with a `${...}` substitution — the
data-feeder case), each call materializes the message repeatedly: the rendered
JSON tree is deep-cloned into `DynamicMessage::deserialize`, the built message
is traversed once by `encoded_len()` purely to compute `bytes_sent`, and then
tonic's codec traverses it again to actually encode it. The literal path
already avoids all of this via the per-`Arc` encoded-body cache (`71fcbe9`);
rendered messages can get most of the same treatment without a cache: drop the
clone (serde can deserialize from `&Value`), encode exactly once at execute
time, take `bytes_sent` from the buffer length, and hand the codec
pre-encoded bytes.

Paste this whole block into a fresh coding-agent session:

```text
/goal Stop deep-cloning rendered JSON into DynamicMessage and encode each rendered gRPC message exactly once, reusing the bytes for bytes_sent and submission

CONTEXT
- Base branch: origin/nikolai/perf-grpc-call-cache-memoization at 1c3d683
  (the gRPC request-prep lineage: 71fcbe9 call/encoded-body cache, a09309c,
  URL/metadata memoization). Line numbers below are 1c3d683's. risotto
  contains this lineage but grpc.rs drifted after (raw transport, lazy
  decode) — anchor by symbol when rebasing; the outbound path is
  conceptually unchanged there.
- Outbound plumbing (crates/loadr-protocols/src/grpc.rs): enum Outbound
  { Dynamic(DynamicMessage), Encoded(Bytes) } (:91); DynamicEncoder encodes
  Dynamic via prost into the EncodeBuf and copies Encoded bytes straight in
  (:115-131). The gRPC framing (the +5 bytes: compression flag + u32 length)
  is added by tonic outside the Encoder.
- Per-call cost today, GrpcHandler::execute (:562):
  - literal messages (:675-719): served from the per-Arc EncodedMessages
    cache as Outbound::Encoded — already encode-once, leave untouched;
  - rendered messages: DynamicMessage::deserialize(cached.input_desc.clone(),
    json.clone()) — a full deep clone of the rendered serde_json tree per
    message (:696 unary, :735 streaming); then
    bytes_sent = Σ encoded_len() + 5 — a full protobuf traversal per message
    just for the metric (:745); then the codec encodes each message again at
    submission (Outbound::Dynamic collected at :746, encoded at :121).
  - Net per rendered message: one avoidable deep clone + two traversals
    where one suffices.
- Why the clone is avoidable: DynamicMessage::deserialize is generic over
  serde::Deserializer and serde_json implements Deserializer for
  &serde_json::Value — deserialize(desc, &*json) borrows; the clone at :696
  and :735 exists only because the value was passed by value.
- The trade in encode-once: today Dynamic encodes directly into tonic's
  EncodeBuf; pre-encoding produces the bytes once but the Encoded arm then
  copies them into the EncodeBuf (:124-127). So the change swaps
  {json deep clone + encoded_len traversal + in-place encode} for
  {one encode + one memcpy}. For any non-trivial message that is a clear
  win; the A/B's small-message case guards the degenerate end.
- No user-visible timing change: rendering, encoding, and submission all
  happen inside the handler's execute window either way.
- Metadata rider (optional, small): CachedMetadata::for_request re-compares
  the full rendered metadata pair list per call and rebuilds on mismatch
  (:220, :246-270) — for templated metadata values that is a guaranteed
  per-call rebuild; for static metadata it is a compare + MetadataMap clone.
  If grpc-template-precompile has landed on the branch, its compile-time
  knowledge that all metadata is literal can skip the compare entirely
  (always-hit). Implement only if that flag exists on your base; otherwise
  leave CachedMetadata untouched and note the residual.
- Evidence status: code inspection; the double traversal scales with message
  size × call rate. The A/B below is the arbiter.

IMPLEMENTATION
- Kill the clone first (independent, safe): pass &*json (or &Value) to
  DynamicMessage::deserialize at :696 and :735. No behavior change; keep the
  error mapping identical.
- Encode once: after building each rendered DynamicMessage, encode it to
  Bytes immediately (prost Message::encode into a BytesMut sized by
  encoded_len(), or encode_to_vec — one traversal either way),
  set bytes_sent from Σ bytes.len() + 5 (identical value to today's formula
  by prost's contract), and submit Outbound::Encoded(bytes) — the same
  variant the literal cache uses (:719), so the codec path is already
  proven.
- Retire Outbound::Dynamic if nothing constructs it afterwards (literal path
  uses Encoded, rendered path now does too): remove the variant and its
  encoder arm (:121-123) rather than keeping dead code. Keep DynamicDecoder
  and the response side untouched.
- Streaming (`messages`) gets the same treatment per element (:735-746).
- Do not add a cache for rendered bytes: rendered messages differ per call
  by construction; the literal cache already covers the repeat case. If a
  message renders identically every call, that is a plan-authoring issue,
  not a handler concern.
- Metadata rider (only with template-precompile's metadata_literal on the
  branch): when the compiled request marks metadata fully literal, build the
  MetadataMap once on first use and skip CachedMetadata::matches thereafter;
  templated metadata keeps today's compare-and-rebuild. Document the
  residual: per-call-changing metadata values inherently re-validate.

OUT OF SCOPE
- The template render path itself (sibling goal grpc-template-precompile;
  neither goal blocks the other — coordinate only on the optional metadata
  rider).
- Response decode, lazy decode, raw transport, channel pooling, reflection.
- prost-reflect/tonic version bumps; any codec wire-format change (the
  encoded bytes must be byte-identical).

CORRECTNESS TESTS
- Byte parity: for a corpus covering all scalar kinds, nested messages,
  repeated fields, enums, and bytes fields (the testserver echo proto has a
  bytes payload field since 7405efa), assert the pre-encoded Bytes equal
  what the base Dynamic path produced (encode via prost in the test as the
  oracle) and that the server-observed request is unchanged (echo round-trip
  through testsupport/loadr-testserver's gRPC echo).
- bytes_sent parity: new value equals the base encoded_len()+5 formula for
  unary and multi-message streaming requests.
- Literal path untouched: the encoded-body cache tests from 71fcbe9 in
  crates/loadr-protocols/tests/integration.rs stay green, and a literal
  message still submits the cached Bytes (no re-encode — assert via the
  cache's hit behavior, not timing).
- Error parity: a rendered message that violates the schema fails
  DynamicMessage::deserialize with the same error surface as base (the
  borrow change must not alter error text).
- If the metadata rider is implemented: static metadata builds its
  MetadataMap once (assert via for_request call/rebuild counting on the
  cached struct), templated metadata still rebuilds per call, and the
  1c3d683 CachedMetadata tests stay green.

LOCAL PERFORMANCE VALIDATION (required; shared harness with grpc-template-precompile)
- Two release binaries (cargo +1.93.0), base 1c3d683 vs candidate (if
  template-precompile is also in flight, A/B this change against a base that
  already contains it, and say so).
- Server: loadr-testserver gRPC echo, loopback, TLS off. Plans: templated
  unary echo at three message sizes — small (~200 B), medium (~2 KB), large
  (~64 KB, exercise the bytes field) — plus one streaming-messages case.
  Closed-model ladder the host sustains; no sample-consuming output.
- ≥5 paired alternating runs per size after warm-up; capture iterations/s,
  wall time, perf stat instructions + task-clock. Expect the win to grow
  with message size; report the small-size case even if flat or slightly
  negative (the memcpy trade), raw + median + dispersion.

QUALITY BAR
Focused correctness tests and the local release A/B as above; no unrelated
refactors; conventional commit, no Claude-Session trailer. Run cargo fmt --all
and cargo clippy --workspace --all-targets -- -D warnings, then cargo test -p
loadr-protocols -p loadr-core --locked (workspace suite before the PR:
--workspace --locked --exclude loadr-browser). Use a current stable toolchain
capable of building the locked dependencies.

DONE when: code inspection finds no json.clone() into deserialize, no
encoded_len()-for-metrics traversal, and at most one encode per rendered
message per call; Outbound::Dynamic is gone or provably still required; byte
and bytes_sent parity tests pass against the base oracle including streaming
and bytes fields; and the paired A/B table across the three message sizes is
attached with the small-message trade reported honestly.
```
