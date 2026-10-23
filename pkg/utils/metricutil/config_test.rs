// Copyright 2026 AsterSQL.
use super::*;
#[test]
fn metric_invalid_interval_and_job_defaults() {
    let mut c = MetricConfig {
        push_interval: Duration::from_nanos(-1),
        push_address: "push".into(),
        ..Default::default()
    };
    c.adjust("PD");
    assert_eq!(c.push_interval.as_nanos(), 15000000000);
    assert_eq!(c.push_job, "PD");
    assert_eq!(c.push_address, "push");
    c.push_interval = Duration::from_nanos(1);
    c.adjust("other");
    assert_eq!(c.push_interval.as_nanos(), 1);
    assert_eq!(c.push_job, "PD");
}
