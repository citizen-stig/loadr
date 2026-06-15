# Publishing a plugin

Once a plugin is written (see [Developing a plugin](developing.md)), this is how
its compiled artifacts get built for every platform, attached to a GitHub
Release, and advertised in the **plugin index** so users can run
`loadr plugin install <name>`.

This is automated by the **Publish plugins** workflow
(`.github/workflows/publish-plugins.yml`). You normally only push a tag; the
workflow does the rest.

## What gets built

The workflow discovers every crate under `plugins/loadr-plugin-*` that ships a
`plugin.toml` **and** builds a `cdylib` (`crate-type = ["cdylib"]`). The
`loadr-plugin-webui` service crate has no `plugin.toml` and is skipped.

Each discovered plugin is built for all five targets loadr ships:

| Target triple                  | Runner          | Library file               |
|--------------------------------|-----------------|----------------------------|
| `x86_64-unknown-linux-gnu`     | `ubuntu-latest` | `lib<lib>.so`              |
| `aarch64-unknown-linux-gnu`    | `ubuntu-latest` | `lib<lib>.so`              |
| `x86_64-apple-darwin`          | `macos-latest`  | `lib<lib>.dylib`           |
| `aarch64-apple-darwin`         | `macos-latest`  | `lib<lib>.dylib`           |
| `x86_64-pc-windows-msvc`       | `windows-latest`| `<lib>.dll`                |

`<lib>` is the crate's `[lib] name` (e.g. `loadr_plugin_mongo`).

## Packaging & naming

Each build produces a flat tarball named after the **manifest** plugin name
(`[plugin].name`, e.g. `mongo` — *not* the crate dir `loadr-plugin-mongo`):

```
<name>-<target>.tar.gz          # the cdylib, renamed to the platform `entry`,
                                # plus a plugin.toml whose `entry` matches
<name>-<target>.tar.gz.sha256   # hex SHA-256 of the archive
```

The library inside the archive is renamed to the platform-correct `entry`
(`.so`/`.dylib`/`.dll`), and the bundled `plugin.toml`'s `entry =` line is
rewritten to match, so the archive installs cleanly on any OS.

## The plugin index

`plugins/index.json` (served at
`https://raw.githubusercontent.com/levantar-ai/loadr/main/plugins/index.json`)
is the default catalogue the installer resolves. The workflow regenerates it
from the built artifacts:

```json
{
  "schema": 1,
  "plugins": {
    "mongo": {
      "kind": "protocol",
      "description": "MongoDB protocol: insert/find/update/delete/aggregate/command",
      "latest": "1.0.0",
      "versions": {
        "1.0.0": {
          "min_loadr_abi": "1.0",
          "artifacts": {
            "x86_64-unknown-linux-gnu": {
              "url": "https://github.com/levantar-ai/loadr/releases/download/plugin-v1.0.0/mongo-x86_64-unknown-linux-gnu.tar.gz",
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

Regeneration **merges** into the existing index, so prior plugins, versions and
targets are preserved; `latest` is recomputed as the highest semver per plugin.
The refreshed index is committed back to `main` so the default URL serves it
immediately.

## Cutting a release

1. Land your plugin crate on `main` (with its `plugin.toml`).
2. Set the workspace version if needed (`scripts/set-version.sh <x.y.z>`), and
   make sure the plugin's `plugin.toml` `version` matches.
3. Push a release tag:

   ```bash
   git tag plugin-v1.0.0
   git push origin plugin-v1.0.0
   ```

The tag push triggers a **real** publish: build all targets, attest SLSA
provenance, create/append the `plugin-v1.0.0` GitHub Release with every
`*.tar.gz` + `*.tar.gz.sha256` + `SHA256SUMS`, regenerate `plugins/index.json`,
and commit it to `main`.

Because the enterprise org forces `GITHUB_TOKEN` to read-only, both the Release
upload and the index commit-back authenticate with the `PAT_TOKEN` secret — the
same pattern as `release.yml`.

## Dry run (testing the workflow)

`workflow_dispatch` defaults to a **dry run**: it builds and packages every
plugin for every target (and attests provenance) but creates **no** Release and
pushes **no** index. Use it to validate packaging from a branch:

- *Actions → Publish plugins → Run workflow* → leave `dry_run` checked.

To publish for real from a manual run, uncheck `dry_run` and supply a `tag`
(e.g. `plugin-v1.0.1`).

## Building locally

`scripts/build-plugin.sh` is the same packaging logic CI uses, runnable on your
machine:

```bash
# scripts/build-plugin.sh <crate-dir> <target-triple> [out-dir]
scripts/build-plugin.sh plugins/loadr-plugin-mongo x86_64-unknown-linux-gnu dist
```

It writes `dist/<name>-<target>.tar.gz`, its `.sha256`, and a
`<name>-<target>.meta.json` that `scripts/gen-plugin-index.sh` consumes to build
the index:

```bash
RELEASE_TAG=plugin-v1.0.0 scripts/gen-plugin-index.sh dist plugins/index.json
```
