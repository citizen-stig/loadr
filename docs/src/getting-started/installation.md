# Installation

## Release binaries

Download the archive for your platform from the
[releases page](https://github.com/reaandrew/loadr.io/releases), unpack it and
put `loadr` on your `PATH`:

```bash
curl -sSL https://github.com/reaandrew/loadr.io/releases/latest/download/loadr-x86_64-unknown-linux-gnu.tar.gz | tar xz
sudo mv loadr-*/loadr /usr/local/bin/
loadr version
```

Builds are published for Linux (x86_64, aarch64), macOS (Intel & Apple
Silicon) and Windows.

## Docker

```bash
docker run --rm -v "$PWD:/work" ghcr.io/reaandrew/loadr run /work/test.yaml
```

The image is distroless (no shell), runs as a non-root user, and contains only
the `loadr` binary.

## From source

```bash
git clone https://github.com/reaandrew/loadr.io
cd loadr.io
cargo install --path crates/loadr-cli
```

Rust 1.85+ is required. There are **no system dependencies** — protobuf
compilation happens in-process (protox), TLS is rustls, and the JS engine
(QuickJS) is compiled in.

## Shell completions

```bash
loadr completions bash | sudo tee /etc/bash_completion.d/loadr
loadr completions zsh > "${fpath[1]}/_loadr"
loadr completions fish > ~/.config/fish/completions/loadr.fish
```

## Editor support for test files

Generate the JSON Schema once and point your editor at it for autocomplete
and inline validation — see [JSON Schema & editor setup](../reference/json-schema.md):

```bash
loadr schema > loadr.schema.json
```
