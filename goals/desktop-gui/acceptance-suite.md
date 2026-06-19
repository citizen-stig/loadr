# loadr Desktop — acceptance test suite (expands M6)

This is an **authoritative requirement** for the desktop build: a full
Playwright-for-Electron suite that **drives the real application** through every
user flow and asserts the outcome — not smoke tests. It supersedes the brief
"e2e" line in `build-electron-gui.md` M6.

## How it runs
- **Playwright `_electron.launch()`** against the built app (`out/`), one window.
- **Headless in CI** via `xvfb-run` on Linux (the matrix's Linux leg gates the
  suite); locally on any platform with a display. Recorded as the
  `desktop-e2e` CI job.
- A **local target** for run/results tests: spawn a throwaway HTTP server (or
  reuse the repo's `testsupport`/`httpbin` harness) so runs hit something real
  and deterministic — never the public internet.
- A **bundled/`$LOADR_BIN` loadr** so the app's validate/schema/run/convert all
  exercise the real CLI.
- **Fixtures** under `desktop/e2e/fixtures/`: a set of plans plus a `.jmx`,
  a k6 `.js`, and a `.har` for import. Deterministic; no time/network flake.
- No arbitrary `sleep`s — assert on observable UI/role/text state with
  Playwright's auto-waiting.

## Flows the suite MUST cover

### Launch & shell
- App launches, window visible, a default "untitled" tab renders with the
  starter plan in both the form pane and the Monaco pane.

### Create & compose (M2/M3)
- New tab → starter renders; edit **name** → the YAML pane shows the new name.
- Add a **scenario**; change **executor** → params reshape (e.g. constant-vus →
  constant-arrival-rate exposes rate/pre_allocated_vus).
- Add a **request** step; set method + URL → reflected in YAML.
- **Drag-and-drop reorder** a flow step (pointer drag) — and the **keyboard**
  path (focus the drag handle, Space, Arrow, Space) — order changes in YAML.
- Remove a step; add nested kinds (group/parallel) and confirm valid YAML.

### Round-trip & validation (M1/M2)
- **Open** each bundled example plan → the form pane renders it; the validation
  badge is green.
- Edit a field → **Save** to a temp path → reopen the file → the change persisted
  and re-renders identically (round-trip through the real CLI).
- Make a deliberately invalid edit in Monaco → the validation strip shows an
  **error** (sourced from `loadr validate`), and a parse error is surfaced
  without losing the text.

### Monaco two-way sync (M2)
- Type YAML in the editor → the form fields update.
- Edit a form field → the Monaco text updates. (Both directions, same doc.)

### Tabs (M3)
- Open several plans; switching tabs **preserves each tab's edits**.
- Dirty indicator appears on edit, clears on save.
- Closing the active tab activates a sensible neighbour; reorder tabs.

### Import (M3)
- Import the `.jmx`, `.js` and `.har` fixtures → each opens as a new tab with a
  converted, **valid** plan; the HAR import shows an auto-correlated `${var}`.

### Run & results (M4)
- Run a plan against the local target → **live metrics** (VUs/RPS/p95/errors)
  appear during the run, then a **results view** (throughput, latency
  percentiles, error rate, thresholds pass/fail, checks).
- A run is added to **history**; **compare** two runs renders a diff.

### Plugins (M5)
- The plugins panel lists installed plugins; install from a local dir and remove
  it (no network), reflected in the list.

### Accessibility & errors
- A keyboard-only pass reaches the primary controls; focus is visible.
- With no loadr binary resolvable, the app shows a clear message rather than
  crashing.

## Definition of done for the suite
Every flow above has at least one passing spec; the `desktop-e2e` CI job is
green on the Linux matrix leg; specs are deterministic (no retries masking
flake). Gaps that genuinely need a display/secret unavailable in CI are recorded
in `desktop/README.md`, not silently skipped.
