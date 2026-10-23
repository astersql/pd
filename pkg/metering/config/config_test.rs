// Copyright 2026 AsterSQL.
use super::*;
#[test]
fn metering_provider_values_and_omitempty() {
    assert_eq!(
        serde_json::to_string(&MeteringConfig::default()).unwrap(),
        "{}"
    );
    for provider in ["aws", "oss", "cos", "azure", "localfs"] {
        let c: MeteringConfig = serde_json::from_str(&format!(r#"{{"{provider}":{{}}}}"#)).unwrap();
        assert_eq!(
            serde_json::to_string(&c).unwrap(),
            format!(r#"{{"{provider}":{{}}}}"#)
        );
    }
    let c: MeteringConfig = serde_json::from_str(
        r#"{"type":"future-provider","aws":{"s3-force-path-style":true},"shared-pool-id":"pool"}"#,
    )
    .unwrap();
    assert_eq!(c.r#type, "future-provider");
    assert!(c.aws.unwrap().s3_force_path_style); // Config.Adjust does not validate provider runtime options.
}
