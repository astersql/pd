// Copyright 2026 AsterSQL.
//! Pushgateway DTO adapter for client_model v0.6.1 metrics.proto (pinned go.mod).
//! Uses the existing prost runtime, preserving the workspace's protobuf-free
//! prometheus collectors. No domain or kvproto types enter this wire adapter.
use prost::Message;
#[derive(Clone, PartialEq, Message)]
struct Pair {
    #[prost(string, optional, tag = "1")]
    name: Option<String>,
    #[prost(string, optional, tag = "2")]
    value: Option<String>,
}
#[derive(Clone, PartialEq, Message)]
struct Value {
    #[prost(double, optional, tag = "1")]
    value: Option<f64>,
}
#[derive(Clone, PartialEq, Message)]
struct Quantile {
    #[prost(double, optional, tag = "1")]
    quantile: Option<f64>,
    #[prost(double, optional, tag = "2")]
    value: Option<f64>,
}
#[derive(Clone, PartialEq, Message)]
struct Summary {
    #[prost(uint64, optional, tag = "1")]
    count: Option<u64>,
    #[prost(double, optional, tag = "2")]
    sum: Option<f64>,
    #[prost(message, repeated, tag = "3")]
    quantiles: Vec<Quantile>,
}
#[derive(Clone, PartialEq, Message)]
struct Bucket {
    #[prost(uint64, optional, tag = "1")]
    count: Option<u64>,
    #[prost(double, optional, tag = "2")]
    bound: Option<f64>,
}
#[derive(Clone, PartialEq, Message)]
struct Histogram {
    #[prost(uint64, optional, tag = "1")]
    count: Option<u64>,
    #[prost(double, optional, tag = "2")]
    sum: Option<f64>,
    #[prost(message, repeated, tag = "3")]
    buckets: Vec<Bucket>,
}
#[derive(Clone, PartialEq, Message)]
struct Metric {
    #[prost(message, repeated, tag = "1")]
    labels: Vec<Pair>,
    #[prost(message, optional, tag = "2")]
    gauge: Option<Value>,
    #[prost(message, optional, tag = "3")]
    counter: Option<Value>,
    #[prost(message, optional, tag = "4")]
    summary: Option<Summary>,
    #[prost(message, optional, tag = "5")]
    untyped: Option<Value>,
    #[prost(int64, optional, tag = "6")]
    timestamp: Option<i64>,
    #[prost(message, optional, tag = "7")]
    histogram: Option<Histogram>,
}
#[derive(Clone, PartialEq, Message)]
struct Family {
    #[prost(string, optional, tag = "1")]
    name: Option<String>,
    #[prost(string, optional, tag = "2")]
    help: Option<String>,
    #[prost(int32, optional, tag = "3")]
    kind: Option<i32>,
    #[prost(message, repeated, tag = "4")]
    metrics: Vec<Metric>,
}

pub(crate) const CONTENT_TYPE: &str =
    "application/vnd.google.protobuf; proto=io.prometheus.client.MetricFamily; encoding=delimited";
pub(crate) fn encode(
    families: &[prometheus::proto::MetricFamily],
) -> Result<Vec<u8>, prost::EncodeError> {
    use prometheus::proto::MetricType;
    let mut bytes = Vec::new();
    for f in families {
        let mut family = Family {
            name: Some(f.name().into()),
            help: Some(f.help().into()),
            kind: Some(f.get_field_type() as i32),
            metrics: Vec::new(),
        };
        for m in f.get_metric() {
            let mut metric = Metric {
                labels: m
                    .get_label()
                    .iter()
                    .map(|l| Pair {
                        name: Some(l.name().into()),
                        value: Some(l.value().into()),
                    })
                    .collect(),
                timestamp: (m.timestamp_ms() != 0).then_some(m.timestamp_ms()),
                ..Default::default()
            };
            match f.get_field_type() {
                MetricType::COUNTER => {
                    metric.counter = Some(Value {
                        value: Some(m.get_counter().get_value()),
                    })
                }
                MetricType::GAUGE => {
                    metric.gauge = Some(Value {
                        value: Some(m.get_gauge().get_value()),
                    })
                }
                MetricType::SUMMARY => {
                    let s = m.get_summary();
                    metric.summary = Some(Summary {
                        count: Some(s.sample_count()),
                        sum: Some(s.sample_sum()),
                        quantiles: s
                            .get_quantile()
                            .iter()
                            .map(|q| Quantile {
                                quantile: Some(q.quantile()),
                                value: Some(q.value()),
                            })
                            .collect(),
                    });
                }
                MetricType::HISTOGRAM => {
                    let h = m.get_histogram();
                    metric.histogram = Some(Histogram {
                        count: Some(h.get_sample_count()),
                        sum: Some(h.get_sample_sum()),
                        buckets: h
                            .get_bucket()
                            .iter()
                            .map(|b| Bucket {
                                count: Some(b.get_cumulative_count()),
                                bound: Some(b.get_upper_bound()),
                            })
                            .collect(),
                    });
                }
                #[allow(deprecated)]
                MetricType::UNTYPED => {
                    metric.untyped = Some(Value {
                        value: Some(m.get_untyped().get_value()),
                    })
                }
            }
            family.metrics.push(metric);
        }
        family.encode_length_delimited(&mut bytes)?;
    }
    Ok(bytes)
}
