//! `loadr plugin` — list, install, enable, disable and inspect plugins.

use std::path::PathBuf;

use clap::Subcommand;
use owo_colors::OwoColorize;

#[derive(Subcommand)]
pub enum PluginCommand {
    /// List discovered plugins
    List {
        /// Plugins directory (default: ~/.loadr/plugins or $LOADR_PLUGINS_DIR)
        #[arg(long)]
        plugins_dir: Option<PathBuf>,
    },
    /// Install a plugin from a directory containing plugin.toml
    Install {
        /// Source directory
        source: PathBuf,
        #[arg(long)]
        plugins_dir: Option<PathBuf>,
    },
    /// Enable a disabled plugin
    Enable {
        name: String,
        #[arg(long)]
        plugins_dir: Option<PathBuf>,
    },
    /// Disable a plugin without removing it
    Disable {
        name: String,
        #[arg(long)]
        plugins_dir: Option<PathBuf>,
    },
    /// Show details for a plugin
    Info {
        name: String,
        #[arg(long)]
        plugins_dir: Option<PathBuf>,
    },
}

fn plugin_type_str(t: &loadr_plugin_api::PluginType) -> &'static str {
    match t {
        loadr_plugin_api::PluginType::Wasm => "wasm",
        loadr_plugin_api::PluginType::Native => "native",
    }
}

fn dir(flag: Option<PathBuf>) -> PathBuf {
    flag.unwrap_or_else(loadr_plugin_api::default_plugins_dir)
}

pub fn execute(cmd: PluginCommand) -> anyhow::Result<i32> {
    match cmd {
        PluginCommand::List { plugins_dir } => {
            let dir = dir(plugins_dir);
            let manifests = match loadr_plugin_api::PluginRegistry::discover(&dir) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("no plugins found in {} ({e})", dir.display());
                    return Ok(0);
                }
            };
            if manifests.is_empty() {
                println!("no plugins installed in {}", dir.display());
                return Ok(0);
            }
            println!(
                "{:<24} {:<10} {:<10} {:<8} {}",
                "NAME".bold(),
                "KIND".bold(),
                "TYPE".bold(),
                "STATE".bold(),
                "VERSION".bold()
            );
            for m in manifests {
                let state = if m.enabled {
                    "enabled".green().to_string()
                } else {
                    "disabled".red().to_string()
                };
                println!(
                    "{:<24} {:<10} {:<10} {:<8} {}",
                    m.name,
                    m.kind.as_str(),
                    plugin_type_str(&m.plugin_type),
                    state,
                    m.version
                );
            }
            Ok(0)
        }
        PluginCommand::Install {
            source,
            plugins_dir,
        } => {
            let dir = dir(plugins_dir);
            let manifest = loadr_plugin_api::PluginRegistry::install_from_dir(&source, &dir)?;
            println!(
                "{} installed `{}` v{} ({}, {}) into {}",
                "✓".green(),
                manifest.name,
                manifest.version,
                manifest.kind.as_str(),
                plugin_type_str(&manifest.plugin_type),
                dir.display()
            );
            Ok(0)
        }
        PluginCommand::Enable { name, plugins_dir } => {
            loadr_plugin_api::PluginRegistry::set_enabled(&dir(plugins_dir), &name, true)?;
            println!("{} `{name}` enabled", "✓".green());
            Ok(0)
        }
        PluginCommand::Disable { name, plugins_dir } => {
            loadr_plugin_api::PluginRegistry::set_enabled(&dir(plugins_dir), &name, false)?;
            println!("{} `{name}` disabled", "✓".green());
            Ok(0)
        }
        PluginCommand::Info { name, plugins_dir } => {
            let dir = dir(plugins_dir);
            let manifests = loadr_plugin_api::PluginRegistry::discover(&dir)?;
            let Some(manifest) = manifests.into_iter().find(|m| m.name == name) else {
                anyhow::bail!("plugin `{name}` is not installed in {}", dir.display());
            };
            println!("{}: {}", "name".bold(), manifest.name);
            println!("{}: {}", "version".bold(), manifest.version);
            println!("{}: {}", "kind".bold(), manifest.kind.as_str());
            println!(
                "{}: {}",
                "type".bold(),
                plugin_type_str(&manifest.plugin_type)
            );
            println!("{}: {}", "entry".bold(), manifest.entry.display());
            println!("{}: {}", "enabled".bold(), manifest.enabled);
            if !manifest.description.is_empty() {
                println!("{}: {}", "description".bold(), manifest.description);
            }
            if !manifest.default_config.is_null() {
                println!(
                    "{}: {}",
                    "default config".bold(),
                    serde_json::to_string_pretty(&manifest.default_config)?
                );
            }
            Ok(0)
        }
    }
}
