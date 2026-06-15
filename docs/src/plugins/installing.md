# Installing plugins

`loadr` ships a small core; extra protocols, outputs and helpers are delivered
as **plugins**. The easiest way to get one is to install it by name from the
**plugin index** — a JSON catalogue that maps a short name to the right
per-platform artifact, with a sha256 for each download.

```bash
loadr plugin install mongo
```

This resolves `mongo` in the index, picks the artifact for your host target
(e.g. `x86_64-unknown-linux-gnu`), checks it against the plugin ABI your
`loadr` build provides, downloads it, verifies its sha256, unpacks it and
installs it into your plugins directory (`~/.loadr/plugins`, or
`$LOADR_PLUGINS_DIR`).

## The index

The default index is the catalogue published on `main`:

```
https://raw.githubusercontent.com/levantar-ai/loadr/main/plugins/index.json
```

Override it with `--index <url>` or the `LOADR_PLUGIN_INDEX` environment
variable (the flag wins). The index format is versioned (`"schema": 1`); an
unknown schema is rejected rather than mis-parsed.

```json
{
  "schema": 1,
  "plugins": {
    "mongo": {
      "kind": "protocol",
      "description": "MongoDB protocol …",
      "latest": "1.0.0",
      "versions": {
        "1.0.0": {
          "min_loadr_abi": "1.0",
          "artifacts": {
            "x86_64-unknown-linux-gnu": {
              "url": "https://…/mongo-x86_64-unknown-linux-gnu.tar.gz",
              "sha256": "…",
              "entry": "libloadr_plugin_mongo.so"
            }
          }
        }
      }
    }
  }
}
```

Each artifact tarball/zip contains a `plugin.toml` and the plugin's dynamic
library. The per-platform artifact filename matters:
`libloadr_plugin_<name>.so` on Linux, `.dylib` on macOS and
`loadr_plugin_<name>.dll` on Windows. After unpacking, `loadr` reconciles the
installed artifact's name with the manifest's `entry`.

## Commands

```bash
# Search the index
loadr plugin search mongo

# Install the latest indexed version for this host
loadr plugin install mongo

# Pin a version / override the host target
loadr plugin install mongo --version 1.0.0 --target aarch64-apple-darwin

# Re-install newer, ABI-compatible versions
loadr plugin update            # every index-managed plugin
loadr plugin update mongo      # just one

# Remove an installed plugin
loadr plugin remove mongo

# List what's installed / inspect one
loadr plugin list
loadr plugin info mongo
```

## ABI compatibility

Every indexed version declares a `min_loadr_abi`. `loadr` refuses to install a
build that needs a newer plugin ABI than the running binary provides, with a
clear message telling you to upgrade `loadr` or pick another version. The
native loader performs the precise `abi_stable` layout check at load time as a
second line of defence.

If the index has no artifact for your target triple, the install fails listing
the targets that *are* available.

## Trust and verification

- **Index installs are the trusted path.** The sha256 in the index is always
  verified after download; a mismatch aborts the install.
- **Other sources require `--allow-untrusted`**, because their integrity is not
  pinned by the official index:

  ```bash
  # A GitHub release's assets (asset matched to the host target triple)
  loadr plugin install github:owner/repo@v1.2.0 --allow-untrusted

  # An arbitrary archive URL or a local archive file
  loadr plugin install https://example.com/myplugin.tar.gz --allow-untrusted
  loadr plugin install ./dist/myplugin.tar.gz --allow-untrusted
  ```

- **A local directory** containing `plugin.toml` installs directly, unchanged
  from earlier `loadr` releases and handy during development:

  ```bash
  loadr plugin install ./dist
  ```

> **Signing (TODO).** sha256 pins integrity today. Signature / SLSA-provenance
> verification of the index and artifacts is a planned hook: the index `schema`
> will carry a signature block and `loadr` will verify it before trusting any
> entry. Until then, the index is trusted by transport (HTTPS to the project's
> repo) and each artifact by its sha256.

## Where plugins live

Installed plugins are directories under the plugins dir, one per plugin:

```text
~/.loadr/plugins/
└── mongo/
    ├── plugin.toml
    └── libloadr_plugin_mongo.so
```

Disable one without removing it (`loadr plugin disable mongo` writes a
`disabled` marker); re-enable with `loadr plugin enable mongo`.
