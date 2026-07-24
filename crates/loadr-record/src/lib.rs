//! Session recorder for loadr.
//!
//! `loadr record` starts a capturing HTTP(S) proxy. Point a browser, app, or
//! `curl` at it, drive the journey you want to load-test, and on shutdown the
//! recorder emits a ready-to-run scenario — with dynamic values (tokens, CSRF,
//! ids) auto-correlated by the same engine that powers `loadr convert har`.
//!
//! Pipeline: capture live traffic → assemble a HAR 1.2 document → hand it to
//! [`loadr_convert::convert_har`] → YAML test plan.

pub mod ca;
pub mod har;
pub mod proxy;

pub use ca::Ca;
pub use har::Recording;
pub use proxy::{run, RecordConfig};

/// What to emit when the recording stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Emit {
    /// A loadr scenario YAML (via the HAR auto-correlator).
    Scenario,
    /// The raw HAR document.
    Har,
}

/// Convert a captured recording into the requested output text.
///
/// Returns `(text, warnings)`. For [`Emit::Scenario`] the warnings are the
/// converter's (e.g. which values were auto-correlated).
pub fn render(recording: &Recording, emit: Emit) -> anyhow::Result<(String, Vec<String>)> {
    let har = recording.to_har();
    match emit {
        Emit::Har => Ok((har, Vec::new())),
        Emit::Scenario => {
            let conversion = loadr_convert::convert_har(&har)
                .map_err(|e| anyhow::anyhow!("converting recording to scenario: {e}"))?;
            let yaml = serde_yaml::to_string(&conversion.plan)?;
            let warnings = conversion
                .warnings
                .iter()
                .map(|w| format!("[{}] {}", w.element, w.message))
                .collect();
            Ok((yaml, warnings))
        }
    }
}
