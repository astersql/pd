// Copyright 2026 AsterSQL.
use astersql_pd_encryption::EncryptionConfig;
use astersql_pd_grpcutil::TLSConfig;
use astersql_pd_logutil::RedactInfoLog;
use serde::{Deserialize, Serialize};
/// Security values without TLS file access or global redaction installation.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SecurityConfig {
    #[serde(flatten)]
    pub tls: TLSConfig,
    #[serde(rename = "redact-info-log")]
    pub redact_info_log: RedactInfoLog,
    #[serde(deserialize_with = "null_default")]
    pub encryption: EncryptionConfig,
}

fn null_default<'de, T: serde::Deserialize<'de> + Default, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<T, D::Error> {
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}
