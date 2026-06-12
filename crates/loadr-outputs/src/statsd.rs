//! StatsD output over UDP, one line per sample.
//!
//! Wire format (`<prefix>` defaults to `loadr.`):
//! - counter → `<prefix><metric>:<value>|c`
//! - gauge → `<prefix><metric>:<value>|g`
//! - trend → `<prefix><metric>:<value>|ms`
//! - rate → `<prefix><metric>:<0|1>|c`
//!
//! Tags use the DogStatsD extension: `|#k:v,k2:v2`. Lines are batched into
//! newline-separated datagrams of at most ~1400 bytes.

use async_trait::async_trait;
use loadr_core::error::EngineError;
use loadr_core::metrics::{MetricKind, Sample};
use loadr_core::output::Output;
use tokio::net::UdpSocket;

const MAX_DATAGRAM: usize = 1400;

/// Sends samples to a StatsD daemon over UDP.
pub struct StatsdOutput {
    address: String,
    prefix: String,
    socket: Option<UdpSocket>,
}

impl StatsdOutput {
    /// Create a StatsD output sending to `address` (e.g. `127.0.0.1:8125`).
    /// `prefix` defaults to `loadr.`.
    pub fn new(address: String, prefix: Option<String>) -> Self {
        StatsdOutput {
            address,
            prefix: prefix.unwrap_or_else(|| "loadr.".to_string()),
            socket: None,
        }
    }

    async fn send(&self, datagram: &str) {
        if let Some(socket) = &self.socket {
            if let Err(err) = socket.send(datagram.as_bytes()).await {
                tracing::warn!(address = %self.address, error = %err, "statsd send failed");
            }
        }
    }
}

fn format_line(prefix: &str, sample: &Sample) -> String {
    let mut line = String::with_capacity(64);
    line.push_str(prefix);
    line.push_str(&sample.metric);
    line.push(':');
    match sample.kind {
        MetricKind::Counter => {
            line.push_str(&sample.value.to_string());
            line.push_str("|c");
        }
        MetricKind::Gauge => {
            line.push_str(&sample.value.to_string());
            line.push_str("|g");
        }
        MetricKind::Trend => {
            line.push_str(&sample.value.to_string());
            line.push_str("|ms");
        }
        MetricKind::Rate => {
            line.push_str(if sample.value != 0.0 { "1" } else { "0" });
            line.push_str("|c");
        }
    }
    if !sample.tags.is_empty() {
        line.push_str("|#");
        for (i, (k, v)) in sample.tags.iter().enumerate() {
            if i > 0 {
                line.push(',');
            }
            line.push_str(k);
            line.push(':');
            line.push_str(v);
        }
    }
    line
}

#[async_trait]
impl Output for StatsdOutput {
    fn name(&self) -> &str {
        "statsd"
    }

    async fn start(&mut self) -> Result<(), EngineError> {
        let bind_addr = if self.address.starts_with('[') {
            "[::]:0"
        } else {
            "0.0.0.0:0"
        };
        let socket = UdpSocket::bind(bind_addr)
            .await
            .map_err(|err| EngineError::Config(format!("statsd: bind local udp socket: {err}")))?;
        socket.connect(&self.address).await.map_err(|err| {
            EngineError::Config(format!("statsd address `{}`: {err}", self.address))
        })?;
        self.socket = Some(socket);
        Ok(())
    }

    async fn on_samples(&mut self, samples: &[Sample]) {
        if self.socket.is_none() {
            return;
        }
        let mut batch = String::with_capacity(MAX_DATAGRAM);
        for sample in samples {
            let line = format_line(&self.prefix, sample);
            if !batch.is_empty() && batch.len() + 1 + line.len() > MAX_DATAGRAM {
                self.send(&batch).await;
                batch.clear();
            }
            if !batch.is_empty() {
                batch.push('\n');
            }
            batch.push_str(&line);
        }
        if !batch.is_empty() {
            self.send(&batch).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::sample;
    use loadr_core::metrics::MetricKind;
    use std::time::Duration;

    async fn recv_datagram(socket: &UdpSocket) -> String {
        let mut buf = vec![0u8; 65536];
        let len = tokio::time::timeout(Duration::from_secs(5), socket.recv(&mut buf))
            .await
            .expect("datagram within timeout")
            .expect("recv");
        String::from_utf8(buf[..len].to_vec()).expect("utf8 datagram")
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn sends_lines_per_kind_with_tags() {
        let receiver = UdpSocket::bind("127.0.0.1:0").await.expect("bind receiver");
        let addr = receiver.local_addr().expect("addr");

        let mut out = StatsdOutput::new(addr.to_string(), None);
        out.start().await.expect("start");
        out.on_samples(&[
            sample("http_reqs", MetricKind::Counter, 2.0, &[("method", "GET")]),
            sample("vus", MetricKind::Gauge, 7.0, &[]),
            sample("http_req_duration", MetricKind::Trend, 12.5, &[]),
            sample("checks", MetricKind::Rate, 1.0, &[("check", "ok")]),
            sample("checks", MetricKind::Rate, 0.0, &[("check", "ok")]),
        ])
        .await;

        let datagram = recv_datagram(&receiver).await;
        let lines: Vec<&str> = datagram.lines().collect();
        assert_eq!(lines.len(), 5, "{datagram}");
        assert_eq!(lines[0], "loadr.http_reqs:2|c|#method:GET");
        assert_eq!(lines[1], "loadr.vus:7|g");
        assert_eq!(lines[2], "loadr.http_req_duration:12.5|ms");
        assert_eq!(lines[3], "loadr.checks:1|c|#check:ok");
        assert_eq!(lines[4], "loadr.checks:0|c|#check:ok");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn batches_split_at_datagram_limit() {
        let receiver = UdpSocket::bind("127.0.0.1:0").await.expect("bind receiver");
        let addr = receiver.local_addr().expect("addr");

        let mut out = StatsdOutput::new(addr.to_string(), Some("p.".to_string()));
        out.start().await.expect("start");
        // Enough lines to exceed one 1400-byte datagram.
        let samples: Vec<_> = (0..100)
            .map(|i| {
                sample(
                    "some_fairly_long_metric_name",
                    MetricKind::Counter,
                    i as f64,
                    &[("zone", "eu-west-2")],
                )
            })
            .collect();
        out.on_samples(&samples).await;

        let first = recv_datagram(&receiver).await;
        assert!(
            first.len() <= MAX_DATAGRAM,
            "datagram too large: {}",
            first.len()
        );
        let second = recv_datagram(&receiver).await;
        let total = first.lines().count() + second.lines().count();
        assert!(total > first.lines().count());
        assert!(first
            .lines()
            .all(|l| l.starts_with("p.some_fairly_long_metric_name:")));
    }
}
