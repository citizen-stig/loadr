//! `loadr gen` — generate a runnable scenario from an API contract.

use std::path::PathBuf;

use clap::Args;
use owo_colors::OwoColorize;

use loadr_gen::GenOptions;

#[derive(Args)]
pub struct GenArgs {
    /// Contract kind
    #[arg(value_parser = ["openapi", "postman", "graphql"])]
    pub source: String,
    /// Contract file (OpenAPI .yaml/.json, or a Postman collection .json)
    pub input: PathBuf,
    /// Output YAML path (default: stdout)
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    /// Override the base URL (default: derived from the spec's `servers`)
    #[arg(long)]
    pub base_url: Option<String>,
    /// Index into the OpenAPI `servers[]` array
    #[arg(long, default_value_t = 0)]
    pub server: usize,
    /// operationId/path globs to include (repeatable; empty = all)
    #[arg(long)]
    pub include: Vec<String>,
    /// operationId/path globs to exclude (repeatable)
    #[arg(long)]
    pub exclude: Vec<String>,
    /// Also emit boundary + spec-invalid + adversarial variants with a "no 5xx" gate
    #[arg(long)]
    pub fuzz: bool,
    /// Adversarial payload kinds to inject when fuzzing (comma-separated)
    #[arg(long, value_delimiter = ',')]
    pub fuzz_payloads: Vec<String>,
}

pub fn execute(args: GenArgs) -> anyhow::Result<i32> {
    let source = std::fs::read_to_string(&args.input)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", args.input.display()))?;

    let opts = GenOptions {
        server: args.server,
        base_url: args.base_url.clone(),
        include: args.include.clone(),
        exclude: args.exclude.clone(),
        fuzz: args.fuzz,
        fuzz_payloads: args.fuzz_payloads.clone(),
    };

    let conversion = match args.source.as_str() {
        "openapi" => loadr_gen::gen_openapi(&source, &opts)?,
        "postman" => loadr_gen::gen_postman(&source, &opts)?,
        "graphql" => loadr_gen::gen_graphql(&source, &opts)?,
        other => anyhow::bail!("unknown source `{other}` (supported: openapi, postman, graphql)"),
    };

    for w in &conversion.warnings {
        eprintln!(
            "{} [{}] {}",
            "warning:".yellow().bold(),
            w.element,
            w.message
        );
    }

    let yaml = serde_yaml::to_string(&conversion.plan)?;
    let header = format!(
        "# Generated from {} by `loadr gen {}` — review and set real load.\n",
        args.input.display(),
        args.source
    );
    match &args.output {
        Some(path) => {
            std::fs::write(path, format!("{header}{yaml}"))?;
            eprintln!("{} wrote {}", "gen:".green().bold(), path.display());
        }
        None => print!("{header}{yaml}"),
    }
    Ok(0)
}
