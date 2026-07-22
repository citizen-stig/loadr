//! Built-in module shims for loadr's JavaScript API.
//!
//! Scripts import loadr's standard library by name — `import http from
//! 'loadr/http'`, `import { check, sleep } from 'loadr'`, and so on. The shims
//! simply re-export the globals installed by the prelude, so the import style
//! and the global style behave identically.
//!
//! The `k6/*` names resolve to the very same shims, purely as a compatibility
//! alias so scripts migrated from k6 (see the migration guide, or `loadr
//! convert`) run unchanged. `loadr/*` is the canonical API; the alias is
//! intentionally undocumented in loadr's own examples.

use rquickjs::loader::{ImportAttributes, Loader, Resolver};
use rquickjs::module::Declared;
use rquickjs::{Ctx, Error, Module};

// The shim sources, one per logical module. Registered under the loadr-native
// names below, and again under the k6 alias names.
// The base `loadr` module is a one-stop specifier: it re-exports the whole
// standard library — the http client and metric classes as well as the
// check/sleep/group/fail helpers — so a script can `import { http, check,
// sleep, Trend } from 'loadr'` instead of reaching for three sub-modules.
const M_BASE: &str = r#"const g = globalThis;
export const http = g.http;
export const check = (...a) => g.check(...a);
export const sleep = (...a) => g.sleep(...a);
export const group = (...a) => g.group(...a);
export function fail(msg) { throw new Error(msg === undefined ? "test failed" : String(msg)); }
export const Counter = g.Counter;
export const Gauge = g.Gauge;
export const Rate = g.Rate;
export const Trend = g.Trend;
export default { http, check, sleep, group, fail, Counter, Gauge, Rate, Trend };
"#;

const M_HTTP: &str = r#"const http = globalThis.http;
export default http;
"#;

const M_METRICS: &str = r#"const g = globalThis;
export const Counter = g.Counter;
export const Gauge = g.Gauge;
export const Rate = g.Rate;
export const Trend = g.Trend;
export default { Counter, Gauge, Rate, Trend };
"#;

const M_CRYPTO: &str = r#"const c = globalThis.crypto;
export const sha256 = c.sha256;
export const sha384 = c.sha384;
export const sha512 = c.sha512;
export const sha1 = c.sha1;
export const md5 = c.md5;
export const hmac = c.hmac;
export const randomBytes = c.randomBytes;
export default c;
"#;

const M_ENCODING: &str = r#"const e = globalThis.encoding;
export const b64encode = e.b64encode;
export const b64decode = e.b64decode;
export default e;
"#;

/// The loadr-native scripting modules — the canonical, documented API.
pub const BUILTIN_MODULES: &[(&str, &str)] = &[
    ("loadr", M_BASE),
    ("loadr/http", M_HTTP),
    ("loadr/metrics", M_METRICS),
    ("loadr/crypto", M_CRYPTO),
    ("loadr/encoding", M_ENCODING),
];

/// k6 module names, resolved to the same shims as a migration-compatibility
/// alias. Not part of loadr's advertised API.
pub const ALIAS_MODULES: &[(&str, &str)] = &[
    ("k6", M_BASE),
    ("k6/http", M_HTTP),
    ("k6/metrics", M_METRICS),
    ("k6/crypto", M_CRYPTO),
    ("k6/encoding", M_ENCODING),
];

fn builtin_source(name: &str) -> Option<&'static str> {
    BUILTIN_MODULES
        .iter()
        .chain(ALIAS_MODULES.iter())
        .find(|(n, _)| *n == name)
        .map(|(_, src)| *src)
}

fn known_modules_hint() -> String {
    // Point users at the loadr-native names only; the k6 aliases stay quiet.
    let names: Vec<&str> = BUILTIN_MODULES.iter().map(|(n, _)| *n).collect();
    format!(
        "unknown module; loadr's built-in modules are: {}",
        names.join(", ")
    )
}

/// Resolves loadr's built-in module names (and the k6 compat aliases) and
/// rejects everything else with a clear message.
pub struct ModuleResolver;

impl Resolver for ModuleResolver {
    fn resolve<'js>(
        &mut self,
        _ctx: &Ctx<'js>,
        base: &str,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<String> {
        if builtin_source(name).is_some() {
            Ok(name.to_string())
        } else {
            Err(Error::new_resolving_message(
                base,
                name,
                known_modules_hint(),
            ))
        }
    }
}

/// Loads the built-in module shims.
pub struct ModuleLoader;

impl Loader for ModuleLoader {
    fn load<'js>(
        &mut self,
        ctx: &Ctx<'js>,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<Module<'js, Declared>> {
        let source = builtin_source(name)
            .ok_or_else(|| Error::new_loading_message(name, known_modules_hint()))?;
        Module::declare(ctx.clone(), name, source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rquickjs::{Context, Runtime};

    #[test]
    fn resolves_native_and_alias_and_rejects_others() {
        let rt = Runtime::new().expect("runtime");
        let ctx = Context::full(&rt).expect("context");
        ctx.with(|ctx| {
            let mut resolver = ModuleResolver;
            // loadr-native names resolve.
            for (name, _) in BUILTIN_MODULES {
                let resolved = resolver
                    .resolve(&ctx, "test.js", name, None)
                    .expect("native module resolves");
                assert_eq!(&resolved, name);
            }
            // k6 aliases still resolve, for migrated scripts.
            for (name, _) in ALIAS_MODULES {
                resolver
                    .resolve(&ctx, "test.js", name, None)
                    .expect("k6 alias resolves");
            }
            // The base `loadr` module re-exports the whole stdlib in one specifier.
            let base = builtin_source("loadr").expect("loadr base module");
            for sym in ["http", "check", "sleep", "group", "Trend", "Counter"] {
                assert!(
                    base.contains(&format!("export const {sym}"))
                        || base.contains(&format!("export function {sym}")),
                    "loadr module re-exports {sym}"
                );
            }

            // Unknown modules are rejected, and the hint names loadr's modules.
            let err = resolver
                .resolve(&ctx, "test.js", "left-pad", None)
                .expect_err("unknown module rejected");
            let msg = err.to_string();
            assert!(msg.contains("left-pad"), "message names the module: {msg}");
            assert!(
                msg.contains("loadr/http"),
                "hint lists loadr modules: {msg}"
            );
        });
    }
}
