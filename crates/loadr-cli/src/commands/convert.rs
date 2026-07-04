//! `loadr convert` — import JMeter .jmx, k6 .js, HAR and access-log files.

use std::path::PathBuf;

use clap::Args;
use owo_colors::OwoColorize;

#[derive(Args)]
pub struct ConvertArgs {
    /// Source file: a JMeter .jmx plan, a k6 .js script, a .har recording, or an access .log
    pub input: PathBuf,
    /// Output YAML path (default: stdout)
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    /// Input kind; inferred from the extension when omitted
    #[arg(long, value_parser = ["jmx", "k6", "har", "accesslog"])]
    pub from: Option<String>,
}

pub fn execute(args: ConvertArgs) -> anyhow::Result<i32> {
    let source = std::fs::read_to_string(&args.input)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", args.input.display()))?;

    let kind = match args.from.as_deref() {
        Some(k) => k.to_string(),
        None => match args.input.extension().and_then(|e| e.to_str()) {
            Some("jmx") | Some("xml") => "jmx".to_string(),
            Some("js") | Some("ts") | Some("mjs") => "k6".to_string(),
            Some("har") => "har".to_string(),
            Some("log") => "accesslog".to_string(),
            other => anyhow::bail!(
                "cannot infer input kind from extension {:?}; pass --from jmx|k6|har|accesslog",
                other
            ),
        },
    };

    let conversion = match kind.as_str() {
        "jmx" => loadr_convert::convert_jmx(&source)?,
        "k6" => loadr_convert::convert_k6(&source)?,
        "har" => loadr_convert::convert_har(&source)?,
        "accesslog" => loadr_convert::convert_accesslog(&source)?,
        _ => unreachable!(),
    };

    for warning in &conversion.warnings {
        eprintln!(
            "{} [{}] {}",
            "warning:".yellow().bold(),
            warning.element,
            warning.message
        );
    }

    let yaml = serde_yaml::to_string(&conversion.plan)?;
    let header = format!(
        "# Converted from {} by `loadr convert` — review warnings before running.\n",
        args.input.display()
    );
    match &args.output {
        Some(path) => {
            std::fs::write(path, format!("{header}{yaml}"))?;
            eprintln!(
                "{} wrote {} ({} warning(s))",
                "✓".green(),
                path.display(),
                conversion.warnings.len()
            );
        }
        None => print!("{header}{yaml}"),
    }
    Ok(0)
}
