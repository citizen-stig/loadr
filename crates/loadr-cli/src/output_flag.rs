//! Parsing for `--output kind=value` flags.

use loadr_config::OutputConfig;

/// Parse `json=path`, `csv=path`, `prometheus=listen_addr`,
/// `influxdb=url,database`, `statsd=addr`, `otlp=endpoint`.
pub fn parse_output_flag(spec: &str) -> Result<OutputConfig, String> {
    let (kind, value) = spec
        .split_once('=')
        .ok_or_else(|| format!("invalid --output `{spec}`; expected kind=value"))?;
    match kind {
        "json" => Ok(OutputConfig::Json { path: value.into() }),
        "csv" => Ok(OutputConfig::Csv { path: value.into() }),
        "prometheus" => Ok(OutputConfig::Prometheus {
            listen: Some(value.to_string()),
            remote_write_url: None,
            interval: None,
        }),
        "influxdb" => {
            let (url, database) = value
                .split_once(',')
                .ok_or_else(|| "influxdb output needs url,database".to_string())?;
            Ok(OutputConfig::Influxdb {
                url: url.to_string(),
                database: database.to_string(),
                token: None,
                organization: None,
                interval: None,
            })
        }
        "statsd" => Ok(OutputConfig::Statsd {
            address: value.to_string(),
            prefix: None,
        }),
        "otlp" => Ok(OutputConfig::Otlp {
            endpoint: value.to_string(),
            protocol: Default::default(),
            headers: Default::default(),
            interval: None,
        }),
        other => Err(format!(
            "unknown output kind `{other}` (json, csv, prometheus, influxdb, statsd, otlp)"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kinds() {
        assert!(matches!(
            parse_output_flag("json=a.jsonl").unwrap(),
            OutputConfig::Json { .. }
        ));
        assert!(matches!(
            parse_output_flag("prometheus=127.0.0.1:9091").unwrap(),
            OutputConfig::Prometheus { .. }
        ));
        assert!(matches!(
            parse_output_flag("influxdb=http://x:8086,db").unwrap(),
            OutputConfig::Influxdb { .. }
        ));
        assert!(parse_output_flag("nope=1").is_err());
        assert!(parse_output_flag("json").is_err());
    }
}
