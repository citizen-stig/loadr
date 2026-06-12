//! Deep-merge for environment overrides.
//!
//! `loadr run -e staging` takes the document under `env.staging` and merges it
//! over the root document: mappings merge recursively, everything else
//! (scalars, sequences) is replaced.

use serde_yaml::Value;

/// Recursively merge `overlay` onto `base` (mappings merge; other values replace).
pub fn deep_merge(base: &mut Value, overlay: &Value) {
    match (base, overlay) {
        (Value::Mapping(base_map), Value::Mapping(overlay_map)) => {
            for (k, v) in overlay_map {
                match base_map.get_mut(k) {
                    Some(existing) => deep_merge(existing, v),
                    None => {
                        base_map.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (base_slot, overlay_value) => {
            *base_slot = overlay_value.clone();
        }
    }
}

/// Apply the named environment override to a parsed YAML document.
///
/// Returns the list of available environment names on failure.
pub fn apply_env(doc: &mut Value, env_name: &str) -> Result<(), Vec<String>> {
    let envs = doc
        .get("env")
        .and_then(|v| v.as_mapping())
        .map(|m| {
            m.keys()
                .filter_map(|k| k.as_str().map(String::from))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let overlay = doc
        .get("env")
        .and_then(|v| v.get(env_name))
        .cloned()
        .ok_or_else(|| envs.clone())?;

    // Drop the env block so overrides can't recurse, then merge.
    if let Value::Mapping(m) = doc {
        m.remove(Value::String("env".to_string()));
    }
    deep_merge(doc, &overlay);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_nested_mappings() {
        let mut base: Value = serde_yaml::from_str(
            r#"
defaults:
  http:
    base_url: https://prod.example.com
    timeout: 30s
scenarios:
  s: { executor: constant-vus, vus: 10, duration: 1m }
env:
  staging:
    defaults:
      http:
        base_url: https://staging.example.com
"#,
        )
        .unwrap();
        apply_env(&mut base, "staging").unwrap();
        assert_eq!(
            base["defaults"]["http"]["base_url"],
            Value::String("https://staging.example.com".into())
        );
        // Untouched siblings survive.
        assert_eq!(
            base["defaults"]["http"]["timeout"],
            Value::String("30s".into())
        );
        // env block removed.
        assert!(base.get("env").is_none());
    }

    #[test]
    fn sequences_are_replaced_not_merged() {
        let mut base: Value = serde_yaml::from_str(
            r#"
outputs: [ { type: json, path: a.jsonl } ]
env:
  ci:
    outputs: [ { type: csv, path: b.csv } ]
"#,
        )
        .unwrap();
        apply_env(&mut base, "ci").unwrap();
        let outs = base["outputs"].as_sequence().unwrap();
        assert_eq!(outs.len(), 1);
        assert_eq!(outs[0]["type"], Value::String("csv".into()));
    }

    #[test]
    fn unknown_env_reports_available() {
        let mut base: Value = serde_yaml::from_str("env: { staging: {}, prod: {} }").unwrap();
        let err = apply_env(&mut base, "qa").unwrap_err();
        assert!(err.contains(&"staging".to_string()));
        assert!(err.contains(&"prod".to_string()));
    }
}
