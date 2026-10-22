// Copyright 2026 AsterSQL.

use crate::catalog::FOUNDATION_ERRORS;

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
