// Companion script for examples/26-failure-breakdown.yaml.
//
// The scenario `exec` function throws an uncaught exception on roughly one in
// four iterations, so the web UI's failure breakdown panel shows a "Script
// exceptions" group alongside the HTTP-status and failed-check groups. The
// exception message is fixed so every occurrence groups together.

export function maybeThrow() {
  if (Math.random() < 0.25) {
    // A realistic "reading a field off something undefined" bug.
    const payload = undefined;
    return payload.token; // TypeError: cannot read property of undefined
  }
}
