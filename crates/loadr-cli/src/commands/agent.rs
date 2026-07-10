//! `loadr agent` — join a controller and generate load on demand.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Args;
use owo_colors::OwoColorize;

#[derive(Args)]
pub struct AgentArgs {
    /// Controller address to join, e.g. `controller-host:7625`
    #[arg(long, value_name = "HOST:PORT")]
    pub join: String,
    /// Agent name shown in the fleet view (default: hostname)
    #[arg(long)]
    pub name: Option<String>,
    /// Stable agent id (default: generated; set for stable identity across restarts)
    #[arg(long)]
    pub id: Option<String>,
    /// Agent label, `key=value` (repeatable); used for agent targeting
    #[arg(long, value_name = "KEY=VALUE")]
    pub label: Vec<String>,
    /// Working directory for shipped data files
    #[arg(long, default_value = "/tmp/loadr-agent")]
    pub work_dir: PathBuf,
    /// CA bundle to verify the controller's TLS certificate
    #[arg(long)]
    pub tls_ca: Option<PathBuf>,
    /// Client certificate for mTLS
    #[arg(long, requires = "tls_key")]
    pub tls_cert: Option<PathBuf>,
    /// Client private key for mTLS
    #[arg(long, requires = "tls_cert")]
    pub tls_key: Option<PathBuf>,
    /// Override the TLS server name
    #[arg(long)]
    pub tls_domain: Option<String>,
}

pub fn execute(args: AgentArgs) -> anyhow::Result<i32> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let mut labels = HashMap::new();
        for label in &args.label {
            let (k, v) = label
                .split_once('=')
                .ok_or_else(|| anyhow::anyhow!("label `{label}` must be key=value"))?;
            labels.insert(k.to_string(), v.to_string());
        }
        let tls_enabled = args.tls_ca.is_some() || args.tls_cert.is_some();
        let scheme = if tls_enabled { "https" } else { "http" };
        let controller_addr = if args.join.starts_with("http") {
            args.join.clone()
        } else {
            format!("{scheme}://{}", args.join)
        };

        let name = args.name.clone().unwrap_or_else(|| {
            std::env::var("HOSTNAME").unwrap_or_else(|_| "loadr-agent".to_string())
        });

        // Real protocol and JS factories.
        let protocols: loadr_agent::ProtocolFactory = Arc::new(|plan, base_dir| {
            let mut registry = loadr_protocols::builtin_registry(&plan.defaults.http, base_dir)
                .map_err(|e| e.to_string())?;
            // Browser protocol (headless Chrome via CDP); lazy until first use.
            registry.register(Arc::new(
                loadr_browser::BrowserHandler::from_config(&plan.defaults.http)
                    .map_err(|e| e.to_string())?,
            ));
            // Protocol plugins declared in the plan. The controller ships no
            // plugin binaries: they resolve on this host from LOADR_PLUGINS_DIR
            // or ~/.loadr/plugins (or an explicit `path:` in the plan).
            let plugins_dir = loadr_plugin_api::default_plugins_dir();
            for plugin_ref in &plan.plugins {
                if !plugin_ref.enabled {
                    continue;
                }
                let loaded = loadr_plugin_api::PluginRegistry::load_ref(plugin_ref, &plugins_dir)
                    .map_err(|e| format!("plugin `{}`: {e}", plugin_ref.name))?;
                match loaded {
                    loadr_plugin_api::LoadedPlugin::Protocol(handler) => registry.register(handler),
                    other => tracing::warn!(
                        plugin = %plugin_ref.name,
                        kind = %other.kind(),
                        "plugin kind has no agent-side plumbing; ignoring"
                    ),
                }
            }
            Ok(registry)
        });
        let script: loadr_agent::ScriptFactory = Arc::new(|js_config, base_dir| {
            loadr_js::JsEngine::new(js_config, base_dir)
                .map(|e| Arc::new(e) as Arc<dyn loadr_core::ScriptEngine>)
                .map_err(|e| e.to_string())
        });

        let config = loadr_agent::AgentConfig {
            controller_addr: controller_addr.clone(),
            agent_id: args.id.clone(),
            agent_name: name.clone(),
            labels,
            tls: tls_enabled.then(|| loadr_agent::AgentTls {
                ca_pem: args.tls_ca.clone(),
                cert_pem: args.tls_cert.clone(),
                key_pem: args.tls_key.clone(),
                domain: args.tls_domain.clone(),
            }),
            work_dir: args.work_dir.clone(),
            deps: loadr_agent::RunnerDeps {
                protocols,
                script: Some(script),
            },
        };

        eprintln!(
            "{} agent `{name}` joining {controller_addr} (Ctrl-C to leave)",
            "→".cyan()
        );
        let shutdown = tokio_util::sync::CancellationToken::new();
        {
            let shutdown = shutdown.clone();
            tokio::spawn(async move {
                let _ = tokio::signal::ctrl_c().await;
                eprintln!("\nleaving the fleet...");
                shutdown.cancel();
            });
        }
        loadr_agent::Agent::run(config, shutdown).await?;
        Ok(0)
    })
}
