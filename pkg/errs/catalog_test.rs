// Copyright 2026 AsterSQL.

use crate::catalog::{FOUNDATION_ERRORS, SCHEDULING_ERRORS, SERVICE_ERRORS};

#[test]
fn catalog_foundation_domains() {
    // errno.go matches oracle 42a0da684cf28f16a48c74c54575b711031f3c5d.
    // Read actual Go declarations rather than duplicating the Rust table.
    let mut expected = std::collections::BTreeMap::new();
    for line in include_str!("errno.go").lines() {
        let Some((_, declaration)) = line.split_once(r#"= errors.Normalize(""#) else {
            continue;
        };
        let Some((message, code)) = declaration.split_once("\", errors.RFCCodeText(\"") else {
            continue;
        };
        let code = code.strip_suffix("\"))").unwrap();
        if ["common", "tso", "member", "core", "client"].contains(&code.split(':').nth(1).unwrap())
        {
            assert!(expected.insert(code, message).is_none());
        }
    }
    assert_eq!(expected.len(), 37);
    assert_eq!(FOUNDATION_ERRORS.len(), expected.len());
    for definition in FOUNDATION_ERRORS {
        assert_eq!(
            Some(definition.template()),
            expected.remove(definition.code()),
            "{}",
            definition.code()
        );
        let args = ["7", "8", "9"];
        let mut rendered = definition.template().to_owned();
        for argument in args {
            if let Some(index) = rendered.find('%') {
                rendered.replace_range(index..index + 2, argument);
            }
        }
        assert_eq!(
            definition.instantiate(args).to_string(),
            format!("[{}]{rendered}", definition.code())
        );
    }
    assert!(expected.is_empty());
}

#[test]
fn catalog_scheduling_domains() {
    let mut expected = std::collections::BTreeMap::new();
    for line in include_str!("errno.go").lines() {
        let Some((_, declaration)) = line.split_once(r#"= errors.Normalize(""#) else {
            continue;
        };
        let Some((message, code)) = declaration.split_once("\", errors.RFCCodeText(\"") else {
            continue;
        };
        let code = code.strip_suffix("\"))").unwrap();
        if ["operator", "schedule", "checker", "region", "keyspace"]
            .contains(&code.split(':').nth(1).unwrap())
        {
            assert!(expected.insert(code, message).is_none());
        }
    }
    assert_eq!(expected.len(), 51);
    assert_eq!(SCHEDULING_ERRORS.len(), expected.len());
    for definition in SCHEDULING_ERRORS {
        assert_eq!(
            Some(definition.template()),
            expected.remove(definition.code()),
            "{}",
            definition.code()
        );
        let args = ["region-7", "keyspace-8"];
        let mut rendered = definition.template().to_owned();
        for argument in args {
            if let Some(index) = rendered.find('%') {
                rendered.replace_range(index..index + 2, argument);
            }
        }
        let error = definition.instantiate(args);
        assert_eq!(
            error.to_string(),
            format!("[{}]{rendered}", definition.code())
        );
        assert_eq!(error, definition.instantiate(args));
        let wrapped = definition.with_source(args, std::io::Error::other("cause"));
        assert_eq!(
            wrapped.to_string(),
            format!("[{}]{rendered}: cause", definition.code())
        );
        assert_eq!(
            std::error::Error::source(&wrapped).unwrap().to_string(),
            "cause"
        );
    }
    assert!(expected.is_empty());
}

#[test]
fn catalog_service_domains() {
    let mut expected = std::collections::BTreeMap::new();
    for line in include_str!("errno.go").lines() {
        let Some((_, declaration)) = line.split_once(r#"= errors.Normalize(""#) else {
            continue;
        };
        let Some((message, code)) = declaration.split_once("\", errors.RFCCodeText(\"") else {
            continue;
        };
        let code = code.strip_suffix("\"))").unwrap();
        if ["server", "api", "typeutil", "ratelimit"].contains(&code.split(':').nth(1).unwrap()) {
            assert!(expected.insert(code, message).is_none());
        }
    }
    assert_eq!(expected.len(), 11);
    assert_eq!(SERVICE_ERRORS.len(), expected.len());
    for definition in SERVICE_ERRORS {
        assert_eq!(
            Some(definition.template()),
            expected.remove(definition.code()),
            "{}",
            definition.code()
        );
        let args = ["7", "8"];
        let mut rendered = definition.template().to_owned();
        for argument in args {
            if let Some(index) = rendered.find('%') {
                rendered.replace_range(index..index + 2, argument);
            }
        }
        let error = definition.instantiate(args);
        assert_eq!(
            error.to_string(),
            format!("[{}]{rendered}", definition.code())
        );
        assert_eq!(error, definition.instantiate(args));
        assert_ne!(error, definition.instantiate(["different", "8"]));
        let wrapped = definition.with_source(args, std::io::Error::other("cause"));
        assert_eq!(
            wrapped.to_string(),
            format!("[{}]{rendered}: cause", definition.code())
        );
        assert_eq!(
            std::error::Error::source(&wrapped).unwrap().to_string(),
            "cause"
        );
    }
    assert!(expected.is_empty());
    use crate::catalog::{ERR_CLIENT_URL_EMPTY, ERR_CONFIG_ITEM};
    assert_eq!(ERR_CLIENT_URL_EMPTY.code(), "PD:server:ErrClientEmpty");
    assert_eq!(ERR_CONFIG_ITEM.code(), "PD:server:ErrConfiguration");
}

#[test]
fn catalog_infrastructure_domains() {
    use crate::catalog::INFRASTRUCTURE_ERRORS;
    use std::error::Error;

    let oracle = include_str!("errno.go")
        .split("// The third-party project error.")
        .nth(1)
        .unwrap()
        .split("// unsafe recovery errors")
        .next()
        .unwrap();
    let mut expected = std::collections::BTreeMap::new();
    for line in oracle.lines() {
        let Some((_, declaration)) = line.split_once(r#"= errors.Normalize(""#) else {
            continue;
        };
        let (message, code) = declaration.split_once("\", errors.RFCCodeText(\"").unwrap();
        assert!(
            expected
                .insert(code.strip_suffix("\"))").unwrap(), message)
                .is_none()
        );
    }
    assert_eq!(expected.len(), 75);
    assert_eq!(INFRASTRUCTURE_ERRORS.len(), expected.len());
    for definition in INFRASTRUCTURE_ERRORS {
        assert_eq!(
            Some(definition.template()),
            expected.remove(definition.code())
        );
        let args = ["detail"];
        let rendered = definition
            .template()
            .replace("%s", "detail")
            .replace("%v", "detail");
        let inner = crate::catalog::ERR_IO_READ.with_source(
            std::iter::empty::<String>(),
            std::io::Error::other("disk failure"),
        );
        let wrapped = definition.with_source(args, inner);
        assert_eq!(
            wrapped.to_string(),
            format!(
                "[{}]{rendered}: [PD:ioutil:ErrIORead]IO read error: disk failure",
                definition.code()
            )
        );
        let source = wrapped.source().unwrap();
        assert!(source.downcast_ref::<crate::TypedError>().is_some());
        assert_eq!(
            source
                .source()
                .unwrap()
                .downcast_ref::<std::io::Error>()
                .unwrap()
                .kind(),
            std::io::ErrorKind::Other
        );
        assert!(source.source().unwrap().source().is_none());
        assert_eq!(definition.instantiate(args), definition.instantiate(args));
        assert_ne!(wrapped, definition.instantiate(args));
    }
    assert!(expected.is_empty());
    assert_eq!(
        crate::catalog::ERR_ENCRYPTION_KMS.code(),
        "PD:ErrEncryptionKMS"
    );
}

#[test]
fn catalog_completeness() {
    use crate::catalog::{DOMAIN_ERROR_GROUPS, LOG_ADAPTER_EXPORTS, SERVER_TRANSPORT_ERRORS};
    use std::collections::BTreeMap;
    let mut expected = BTreeMap::new();
    let mut transport = Vec::new();
    for line in include_str!("errno.go").lines() {
        let line = line.trim();
        if line.starts_with("Err") {
            let (name, value) = line.split_once(" = ").unwrap_or_else(|| {
                let (name, value) = line.split_once('=').unwrap();
                (name.trim(), value.trim())
            });
            let name = name.trim();
            if let Some(value) = value.strip_prefix("errors.Normalize(\"") {
                let (message, code) = value.split_once("\", errors.RFCCodeText(\"").unwrap();
                assert!(
                    expected
                        .insert(code.strip_suffix("\"))").unwrap(), message)
                        .is_none()
                );
            } else {
                assert!(
                    value.starts_with("status.Error(") || value.starts_with("func("),
                    "unclassified {name}"
                );
                transport.push(name);
            }
        }
    }
    assert_eq!(transport, SERVER_TRANSPORT_ERRORS);
    assert_eq!(transport.len(), 15);
    let count = expected.len();
    for group in DOMAIN_ERROR_GROUPS {
        for error in *group {
            assert_eq!(
                expected.remove(error.code()),
                Some(error.template()),
                "{}",
                error.code()
            );
        }
    }
    assert!(expected.is_empty(), "missing: {expected:?}");
    assert_eq!(count, 239);
    assert_eq!(LOG_ADAPTER_EXPORTS, &["ZapError"]);
    assert!(include_str!("errs.go").contains("func ZapError("));
    println!(
        "audited {count} domain errors, {} transport errors, {} log adapter",
        transport.len(),
        LOG_ADAPTER_EXPORTS.len()
    );
}
