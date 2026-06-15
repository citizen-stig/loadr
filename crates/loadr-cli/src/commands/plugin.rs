//! `loadr plugin` — list, install, search, update, remove, enable, disable
//! and inspect plugins.
//!
//! `install` accepts several source forms:
//!
//! - a **short name** (`mongo`) — resolved against the signed plugin index,
//!   matched to the host target triple + ABI, downloaded, sha256-verified and
//!   installed. This is the trusted path.
//! - a **local directory** containing `plugin.toml` (the original behaviour).
//! - `github:owner/repo[@tag]` — installs the release asset matching the host
//!   target. Requires `--allow-untrusted` (not the official index).
//! - an **https URL** or **local archive file** (`.tar.gz`/`.tgz`/`.zip`).
//!   Requires `--allow-untrusted`.

use std::path::{Path, PathBuf};

use clap::Subcommand;
use loadr_plugin_api::{host_target, index_url, Fetcher as _, PluginIndex};
use owo_colors::OwoColorize;

use super::download::{resolve_github, HttpFetcher};

#[derive(Subcommand)]
pub enum PluginCommand {
    /// List discovered plugins
    List {
        /// Plugins directory (default: ~/.loadr/plugins or $LOADR_PLUGINS_DIR)
        #[arg(long)]
        plugins_dir: Option<PathBuf>,
    },
    /// Install a plugin from the index (by name), a directory, a URL,
    /// a local archive, or `github:owner/repo[@tag]`
    #[command(disable_version_flag = true)]
    Install {
        /// Plugin name, directory, archive URL/file, or `github:owner/repo`
        source: String,
        /// Pin a specific version (index installs only)
        #[arg(long)]
        version: Option<String>,
        /// Override the host target triple (e.g. `aarch64-apple-darwin`)
        #[arg(long)]
        target: Option<String>,
        /// Override the plugin index URL ($LOADR_PLUGIN_INDEX, then default)
        #[arg(long)]
        index: Option<String>,
        /// Allow installing from non-index sources (URL / file / github)
        #[arg(long)]
        allow_untrusted: bool,
        #[arg(long)]
        plugins_dir: Option<PathBuf>,
    },
    /// Search the plugin index
    Search {
        term: String,
        #[arg(long)]
        index: Option<String>,
    },
    /// Re-install newer, ABI-compatible versions from the index
    Update {
        /// Plugin name; omit to update every index-managed installed plugin
        name: Option<String>,
        #[arg(long)]
        target: Option<String>,
        #[arg(long)]
        index: Option<String>,
        #[arg(long)]
        plugins_dir: Option<PathBuf>,
    },
    /// Remove an installed plugin
    Remove {
        name: String,
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

/// Classify an `install` source argument.
enum Source {
    Dir(PathBuf),
    Url(String),
    File(PathBuf),
    Github(String),
    Name(String),
}

fn classify(source: &str) -> Source {
    if let Some(rest) = source.strip_prefix("github:") {
        return Source::Github(rest.to_string());
    }
    if source.starts_with("https://") || source.starts_with("http://") {
        return Source::Url(source.to_string());
    }
    let path = Path::new(source);
    if path.is_dir() {
        return Source::Dir(path.to_path_buf());
    }
    if path.is_file() {
        return Source::File(path.to_path_buf());
    }
    // A bare token with a path separator that doesn't exist is most likely a
    // mistyped path; otherwise treat it as an index name.
    if source.contains('/') || source.contains(std::path::MAIN_SEPARATOR) {
        return Source::Dir(path.to_path_buf());
    }
    Source::Name(source.to_string())
}

fn print_installed(manifest: &loadr_plugin_api::PluginManifest, dest: &Path) {
    println!(
        "{} installed `{}` v{} ({}, {}) into {}",
        "✓".green(),
        manifest.name,
        manifest.version,
        manifest.kind.as_str(),
        plugin_type_str(&manifest.plugin_type),
        dest.display()
    );
}

fn fetch_index(fetcher: &HttpFetcher, index: Option<&str>) -> anyhow::Result<PluginIndex> {
    let url = index_url(index);
    let bytes = fetcher.fetch(&url).map_err(anyhow::Error::from)?;
    Ok(PluginIndex::parse(&bytes)?)
}

#[allow(clippy::too_many_lines)]
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
            version,
            target,
            index,
            allow_untrusted,
            plugins_dir,
        } => {
            let dir = dir(plugins_dir);
            let target = target.unwrap_or_else(|| host_target().to_string());
            match classify(&source) {
                Source::Dir(path) => {
                    let manifest = loadr_plugin_api::PluginRegistry::install_from_dir(&path, &dir)?;
                    print_installed(&manifest, &dir);
                }
                Source::Name(name) => {
                    let fetcher = HttpFetcher::new()?;
                    let idx = fetch_index(&fetcher, index.as_deref())?;
                    let resolved = idx.resolve(&name, version.as_deref(), &target)?;
                    println!(
                        "resolving `{}` v{} for {} from the index…",
                        resolved.name, resolved.version, resolved.target
                    );
                    let manifest = loadr_plugin_api::install_resolved(&resolved, &fetcher, &dir)?;
                    print_installed(&manifest, &dir);
                }
                Source::Github(spec) => {
                    require_untrusted(allow_untrusted, "github:")?;
                    let fetcher = HttpFetcher::new()?;
                    let (url, name) = resolve_github(&fetcher, &spec, &target)?;
                    println!("downloading {name} from github:{spec}…");
                    let bytes = fetcher.fetch(&url)?;
                    let manifest = loadr_plugin_api::install_archive_bytes(&name, &bytes, &dir)?;
                    print_installed(&manifest, &dir);
                }
                Source::Url(url) => {
                    require_untrusted(allow_untrusted, "a URL")?;
                    let fetcher = HttpFetcher::new()?;
                    println!("downloading {url}…");
                    let bytes = fetcher.fetch(&url)?;
                    let manifest = loadr_plugin_api::install_archive_bytes(&url, &bytes, &dir)?;
                    print_installed(&manifest, &dir);
                }
                Source::File(path) => {
                    require_untrusted(allow_untrusted, "a local archive")?;
                    let bytes = std::fs::read(&path)?;
                    let name = path.to_string_lossy();
                    let manifest = loadr_plugin_api::install_archive_bytes(&name, &bytes, &dir)?;
                    print_installed(&manifest, &dir);
                }
            }
            Ok(0)
        }
        PluginCommand::Search { term, index } => {
            let fetcher = HttpFetcher::new()?;
            let idx = fetch_index(&fetcher, index.as_deref())?;
            let hits = idx.search(&term);
            if hits.is_empty() {
                println!("no plugins matching `{term}`");
                return Ok(0);
            }
            println!(
                "{:<20} {:<10} {:<10} {}",
                "NAME".bold(),
                "KIND".bold(),
                "LATEST".bold(),
                "DESCRIPTION".bold()
            );
            for name in hits {
                let entry = &idx.plugins[name];
                println!(
                    "{:<20} {:<10} {:<10} {}",
                    name, entry.kind, entry.latest, entry.description
                );
            }
            Ok(0)
        }
        PluginCommand::Update {
            name,
            target,
            index,
            plugins_dir,
        } => {
            let dir = dir(plugins_dir);
            let target = target.unwrap_or_else(|| host_target().to_string());
            let fetcher = HttpFetcher::new()?;
            let idx = fetch_index(&fetcher, index.as_deref())?;
            let installed = loadr_plugin_api::PluginRegistry::discover(&dir)?;
            let names: Vec<String> = match name {
                Some(n) => vec![n],
                None => installed.iter().map(|m| m.name.clone()).collect(),
            };
            if names.is_empty() {
                println!("no plugins installed in {}", dir.display());
                return Ok(0);
            }
            let mut updated = 0;
            for n in names {
                let Some(current) = installed.iter().find(|m| m.name == n) else {
                    eprintln!("{} `{n}` is not installed; skipping", "!".yellow());
                    continue;
                };
                let resolved = match idx.resolve(&n, None, &target) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("{} cannot update `{n}`: {e}", "!".yellow());
                        continue;
                    }
                };
                if resolved.version == current.version {
                    println!("`{n}` is up to date (v{})", current.version);
                    continue;
                }
                println!(
                    "updating `{n}` v{} -> v{}…",
                    current.version, resolved.version
                );
                let manifest = loadr_plugin_api::install_resolved(&resolved, &fetcher, &dir)?;
                print_installed(&manifest, &dir);
                updated += 1;
            }
            println!("{updated} plugin(s) updated");
            Ok(0)
        }
        PluginCommand::Remove { name, plugins_dir } => {
            let dir = dir(plugins_dir);
            if loadr_plugin_api::remove(&dir, &name)? {
                println!("{} removed `{name}`", "✓".green());
            } else {
                anyhow::bail!("plugin `{name}` is not installed in {}", dir.display());
            }
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

fn require_untrusted(allow: bool, what: &str) -> anyhow::Result<()> {
    if allow {
        Ok(())
    } else {
        anyhow::bail!(
            "installing from {what} is not an official-index source; \
             re-run with --allow-untrusted to proceed (sha256 is not pinned for this source)"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_sources() {
        assert!(matches!(classify("mongo"), Source::Name(n) if n == "mongo"));
        assert!(matches!(
            classify("github:levantar-ai/loadr"),
            Source::Github(_)
        ));
        assert!(matches!(
            classify("https://x.test/p.tar.gz"),
            Source::Url(_)
        ));
        // A name containing a slash but not existing is treated as a path.
        assert!(matches!(classify("./does-not-exist"), Source::Dir(_)));
    }

    #[test]
    fn untrusted_gate() {
        assert!(require_untrusted(true, "a URL").is_ok());
        assert!(require_untrusted(false, "a URL").is_err());
    }
}
