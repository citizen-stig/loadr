//! `loadr validate` — lint test files with precise diagnostics.

use std::path::PathBuf;

use clap::Args;
use owo_colors::OwoColorize;

#[derive(Args)]
pub struct ValidateArgs {
    /// Test files to validate
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Environment override to apply before validating
    #[arg(short, long)]
    pub env: Option<String>,
    /// Skip checking that referenced files (CSV, JS, protos) exist
    #[arg(long)]
    pub no_check_files: bool,
    /// Output format
    #[arg(long, value_parser = ["text", "json"], default_value = "text")]
    pub format: String,
}

pub fn execute(args: ValidateArgs) -> anyhow::Result<i32> {
    let mut total_errors = 0usize;
    let mut total_warnings = 0usize;
    let mut json_out = Vec::new();

    for file in &args.files {
        let mut opts = loadr_config::LoadOptions::new();
        opts.env = args.env.clone();
        opts.check_files = !args.no_check_files;
        opts.deny_errors = false;

        let diagnostics = match loadr_config::load_file(file, &opts) {
            Ok(loaded) => {
                if args.format == "text" {
                    let errors = loaded
                        .diagnostics
                        .iter()
                        .filter(|d| d.severity == loadr_config::Severity::Error)
                        .count();
                    if errors == 0 {
                        let scenarios = loaded.plan.scenarios.len();
                        let requests = count_requests(&loaded.plan);
                        println!(
                            "{} {} is valid ({scenarios} scenario(s), {requests} request(s))",
                            "✓".green(),
                            file.display()
                        );
                    }
                }
                loaded.diagnostics
            }
            Err(loadr_config::ConfigError::Deserialize(diag)) => vec![diag],
            Err(loadr_config::ConfigError::Invalid(diags)) => diags,
            Err(e) => {
                total_errors += 1;
                if args.format == "json" {
                    json_out.push(serde_json::json!({
                        "file": file.display().to_string(),
                        "severity": "error",
                        "message": e.to_string(),
                    }));
                } else {
                    eprintln!("{}: {}: {e}", "error".red().bold(), file.display());
                }
                continue;
            }
        };

        for d in &diagnostics {
            match d.severity {
                loadr_config::Severity::Error => total_errors += 1,
                loadr_config::Severity::Warning => total_warnings += 1,
            }
            if args.format == "json" {
                let mut v = serde_json::to_value(d)?;
                if let Some(obj) = v.as_object_mut() {
                    obj.insert(
                        "file".into(),
                        serde_json::Value::String(file.display().to_string()),
                    );
                }
                json_out.push(v);
            } else {
                let line = d.to_string();
                match d.severity {
                    loadr_config::Severity::Error => eprintln!("{}", line.red()),
                    loadr_config::Severity::Warning => eprintln!("{}", line.yellow()),
                }
            }
        }
    }

    if args.format == "json" {
        println!("{}", serde_json::to_string_pretty(&json_out)?);
    } else if total_errors + total_warnings > 0 {
        eprintln!("{total_errors} error(s), {total_warnings} warning(s)");
    }
    Ok(if total_errors > 0 { 1 } else { 0 })
}

fn count_requests(plan: &loadr_config::TestPlan) -> usize {
    use loadr_config::Step;
    fn steps(list: &[Step]) -> usize {
        list.iter()
            .map(|s| match s {
                Step::Request(_) => 1,
                Step::Group(g) => steps(&g.steps),
                Step::Repeat(r) => steps(&r.steps),
                Step::While(w) => steps(&w.steps),
                Step::If(c) => steps(&c.then) + steps(&c.otherwise),
                Step::Random(r) => r.choices.iter().map(|c| steps(&c.steps)).sum(),
                Step::ThinkTime(_) | Step::Js(_) => 0,
            })
            .sum()
    }
    plan.scenarios.values().map(|s| steps(&s.flow)).sum()
}
