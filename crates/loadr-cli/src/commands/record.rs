//! `loadr record` — capture live HTTP(S) traffic through a proxy and emit a
//! ready-to-run, auto-correlated loadr scenario.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Args;
use owo_colors::OwoColorize;

use loadr_record::{ca::Ca, Emit, Recording};

#[derive(Args)]
pub struct RecordArgs {
    /// Address the recording proxy listens on
    #[arg(short, long, default_value = "127.0.0.1:8888")]
    pub listen: SocketAddr,

    /// Where to write the result (default: stdout)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Emit the raw HAR document instead of a loadr scenario
    #[arg(long)]
    pub har: bool,

    /// Print the CA certificate location + trust instructions and exit
    #[arg(long)]
    pub trust: bool,

    /// Override the directory holding the recorder CA
    #[arg(long)]
    pub ca_dir: Option<PathBuf>,
}

pub fn execute(args: RecordArgs) -> anyhow::Result<i32> {
    // rustls needs a process-wide crypto provider for both the MITM server
    // certs and the upstream client.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let ca_dir = args.ca_dir.clone().unwrap_or_else(Ca::default_dir);
    let ca = Ca::load_or_create(&ca_dir)?;
    let cert_path = ca_dir.join("record-ca-cert.pem");

    if args.trust {
        print_trust(&cert_path);
        return Ok(0);
    }

    let recording = Recording::new();
    let emit = if args.har { Emit::Har } else { Emit::Scenario };

    print_banner(&args.listen, &cert_path);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let cfg = loadr_record::RecordConfig {
            listen: args.listen,
            recording: recording.clone(),
            ca: Arc::new(ca),
        };
        tokio::select! {
            r = loadr_record::run(cfg) => { r?; }
            _ = tokio::signal::ctrl_c() => {
                eprintln!("\n{} stopping — {} transaction(s) captured",
                    "record:".red().bold(), recording.len());
            }
        }

        if recording.is_empty() {
            eprintln!(
                "{} nothing captured — was the proxy configured?",
                "warning:".yellow().bold()
            );
            return Ok::<i32, anyhow::Error>(0);
        }

        let (text, warnings) = loadr_record::render(&recording, emit)?;
        for w in &warnings {
            eprintln!("{} {w}", "note:".cyan().bold());
        }
        match &args.output {
            Some(path) => {
                std::fs::write(path, &text)?;
                eprintln!("{} wrote {}", "record:".green().bold(), path.display());
            }
            None => print!("{text}"),
        }
        Ok(0)
    })
}

fn print_banner(listen: &SocketAddr, cert_path: &std::path::Path) {
    eprintln!(
        "{} recording proxy on {}",
        "loadr record".red().bold(),
        listen.to_string().bold()
    );
    eprintln!("  Point your client at it, e.g.:");
    eprintln!(
        "    {}",
        format!("export HTTP_PROXY=http://{listen} HTTPS_PROXY=http://{listen}").dimmed()
    );
    eprintln!(
        "    {}",
        format!("curl -x http://{listen} https://example.com/").dimmed()
    );
    eprintln!(
        "  For HTTPS, trust the CA once: {}",
        format!("loadr record --trust  ({})", cert_path.display()).dimmed()
    );
    eprintln!("  {} to stop and emit the scenario.\n", "Ctrl-C".bold());
}

fn print_trust(cert_path: &std::path::Path) {
    println!("loadr record CA certificate:\n  {}\n", cert_path.display());
    println!("Trust it so the recorder can capture HTTPS (MITM on localhost only):\n");
    println!("  macOS:");
    println!("    sudo security add-trusted-cert -d -r trustRoot \\");
    println!(
        "      -k /Library/Keychains/System.keychain {}\n",
        cert_path.display()
    );
    println!("  Linux (Debian/Ubuntu):");
    println!(
        "    sudo cp {} /usr/local/share/ca-certificates/loadr-record.crt",
        cert_path.display()
    );
    println!("    sudo update-ca-certificates\n");
    println!("  Firefox / Chrome: import it under Settings → Certificates → Authorities.\n");
    println!("Remove it when you're done recording if you prefer — a new one is minted on demand.");
}
