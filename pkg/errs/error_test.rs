// Copyright 2026 AsterSQL.

use std::{error::Error, fmt};

use super::ErrorTemplate;

#[derive(Debug)]
struct SourceError(&'static str);

impl fmt::Display for SourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Error for SourceError {}

#[test]
fn typed_error_core() {
    const NOT_FOUND: ErrorTemplate = ErrorTemplate::new(
        "PD:scheduler:ErrSchedulerNotFound",
        "scheduler %s not found",
    );
    const OTHER: ErrorTemplate =
        ErrorTemplate::new("PD:scheduler:ErrOther", "scheduler %s not found");

    let first = NOT_FOUND.instantiate(["balance-region"]);
    let second = NOT_FOUND.instantiate(["balance-region"]);
    assert_eq!(NOT_FOUND.code(), "PD:scheduler:ErrSchedulerNotFound");
    assert_eq!(NOT_FOUND.template(), "scheduler %s not found");
    assert_eq!(first.code(), "PD:scheduler:ErrSchedulerNotFound");
    assert_eq!(first.template(), "scheduler %s not found");
    assert_eq!(first.args(), &["balance-region"]);
    assert_eq!(
        first.to_string(),
        "[PD:scheduler:ErrSchedulerNotFound]scheduler balance-region not found"
    );
    assert_eq!(first, second);

    assert_ne!(first, NOT_FOUND.instantiate(["balance-leader"]));
    assert_ne!(first, OTHER.instantiate(["balance-region"]));

    let sourced = NOT_FOUND.with_source(["balance-region"], SourceError("backend unavailable"));
    assert_eq!(
        sourced.to_string(),
        "[PD:scheduler:ErrSchedulerNotFound]scheduler balance-region not found: backend unavailable"
    );
    assert_eq!(sourced.source().unwrap().to_string(), "backend unavailable");
    assert_eq!(
        sourced,
        NOT_FOUND.with_source(["balance-region"], SourceError("backend unavailable"))
    );
    assert_ne!(
        sourced,
        NOT_FOUND.with_source(["balance-region"], SourceError("request canceled"))
    );
}
