//! `loadr payload` — generate adversarial payloads for algorithmic-complexity
//! (DoS) testing. Pipe one into a request body, or scale it with `loadr sweep`
//! and `--assert-complexity` to catch a super-linear parser automatically.

use std::io::Write;
use std::path::PathBuf;

use clap::Args;
use owo_colors::OwoColorize;

#[derive(Args)]
pub struct PayloadArgs {
    /// Payload spec: `<kind>` or `<kind>:<magnitude>` (e.g. `nested-json:10000`,
    /// `nested-markdown-blockquote:64000`). Omit when using `--list`.
    #[arg(required_unless_present = "list")]
    pub spec: Option<String>,

    /// Write to a file instead of stdout.
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// List every payload kind with its category, parameter and cap.
    #[arg(long)]
    pub list: bool,
}

pub fn execute(args: PayloadArgs) -> anyhow::Result<i32> {
    if args.list {
        print_catalog();
        return Ok(0);
    }

    let spec = args.spec.expect("clap requires spec unless --list");
    let parsed = loadr_payload::parse_spec(&spec)?;
    let bytes = loadr_payload::generate(&parsed)?;

    match &args.output {
        Some(path) => {
            std::fs::write(path, &bytes)
                .map_err(|e| anyhow::anyhow!("cannot write {}: {e}", path.display()))?;
            let info = loadr_payload::info(&parsed.name);
            eprintln!(
                "{} wrote {} ({} {} {}) — {} bytes to {}",
                "✓".green(),
                parsed.name.bold(),
                parsed.magnitude,
                info.map(|i| i.param).unwrap_or("units"),
                info.map(|i| format!("· {}", i.content_type))
                    .unwrap_or_default(),
                bytes.len(),
                path.display()
            );
        }
        None => {
            // Raw bytes to stdout so it pipes cleanly into curl/xargs.
            std::io::stdout().write_all(&bytes)?;
        }
    }
    Ok(0)
}

fn print_catalog() {
    println!(
        "{}  —  scale the magnitude and watch response time; see `loadr sweep --complexity`\n",
        "loadr payload".bold()
    );
    let mut last = "";
    for p in loadr_payload::CATALOG {
        if p.category != last {
            println!("{}", p.category.to_uppercase().cyan().bold());
            last = p.category;
        }
        println!(
            "  {:<28} {:<7} default {:<10} max {:<12} {}",
            p.name.bold(),
            p.param,
            p.default,
            p.max,
            p.about.dimmed(),
        );
    }
    println!(
        "\n{}  loadr payload nested-markdown-blockquote:64000 -o bomb.md",
        "example:".dimmed()
    );
}
