// Copyright 2026 AsterSQL.
use crate::mcs_config::Config;
#[test]
fn scheduling_discovery_requires_an_explicit_valid_deployment_policy() {
    assert_eq!(
        Config::parse("scheduling", vec![]).unwrap().pd_discovery,
        "etcd-leader"
    );
    assert_eq!(
        Config::parse("scheduling", vec!["--pd-discovery=primary-key".into()])
            .unwrap()
            .pd_discovery,
        "primary-key"
    );
    assert!(Config::parse("scheduling", vec!["--pd-discovery=guess".into()]).is_err());
    for role in ["router", "tso", "resource-manager"] {
        assert!(Config::parse(role, vec!["--pd-discovery=primary-key".into()]).is_err());
    }
}
