//! `loadr report` — render an HTML report from a summary JSON file.

use std::path::PathBuf;

use clap::Args;
use owo_colors::OwoColorize;

#[derive(Args)]
pub struct ReportArgs {
    /// Summary JSON produced by `loadr run --summary-export`
    pub input: PathBuf,
    /// Output HTML path
    #[arg(short, long, default_value = "loadr-report.html")]
    pub output: PathBuf,
}

pub fn execute(args: ReportArgs) -> anyhow::Result<i32> {
    let raw = std::fs::read_to_string(&args.input)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", args.input.display()))?;
    let summary: loadr_core::Summary = serde_json::from_str(&raw).map_err(|e| {
        anyhow::anyhow!(
            "{} is not a loadr summary export: {e}",
            args.input.display()
        )
    })?;
    let html = crate::report_html::render(&summary);
    std::fs::write(&args.output, html)?;
    eprintln!("{} wrote {}", "✓".green(), args.output.display());
    Ok(0)
}
