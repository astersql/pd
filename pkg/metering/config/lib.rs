// Copyright 2026 AsterSQL.
//! Pure adapter for pinned metering_sdk/config values; provider IO is excluded.
use serde::{Deserialize, Serialize};
/// Pure values from Go MeteringConfig; no service or IO initialization.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MeteringConfig {
    #[serde(rename = "type", skip_serializing_if = "String::is_empty")]
    #[serde(deserialize_with = "null_default")]
    pub r#type: String,
    #[serde(rename = "region", skip_serializing_if = "String::is_empty")]
    #[serde(deserialize_with = "null_default")]
    pub region: String,
    #[serde(rename = "bucket", skip_serializing_if = "String::is_empty")]
    #[serde(deserialize_with = "null_default")]
    pub bucket: String,
    #[serde(rename = "prefix", skip_serializing_if = "String::is_empty")]
    #[serde(deserialize_with = "null_default")]
    pub prefix: String,
    #[serde(rename = "endpoint", skip_serializing_if = "String::is_empty")]
    #[serde(deserialize_with = "null_default")]
    pub endpoint: String,
    #[serde(rename = "aws", skip_serializing_if = "Option::is_none")]
    pub aws: Option<MeteringAWSConfig>,
    #[serde(rename = "oss", skip_serializing_if = "Option::is_none")]
    pub oss: Option<MeteringOSSConfig>,
    #[serde(rename = "cos", skip_serializing_if = "Option::is_none")]
    pub cos: Option<MeteringCOSConfig>,
    #[serde(rename = "azure", skip_serializing_if = "Option::is_none")]
    pub azure: Option<MeteringAzureConfig>,
    #[serde(rename = "localfs", skip_serializing_if = "Option::is_none")]
    pub local_fs: Option<MeteringLocalFSConfig>,
    #[serde(rename = "shared-pool-id", skip_serializing_if = "String::is_empty")]
    #[serde(deserialize_with = "null_default")]
    pub shared_pool_id: String,
}
/// Pure values from Go MeteringAWSConfig; no service or IO initialization.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MeteringAWSConfig {
    #[serde(rename = "assume-role-arn", skip_serializing_if = "String::is_empty")]
    #[serde(deserialize_with = "null_default")]
    pub assume_role_arn: String,
    #[serde(rename = "s3-force-path-style", skip_serializing_if = "is_false")]
    #[serde(deserialize_with = "null_default")]
    pub s3_force_path_style: bool,
    #[serde(rename = "access-key", skip_serializing_if = "String::is_empty")]
    #[serde(deserialize_with = "null_default")]
    pub access_key: String,
    #[serde(rename = "secret-access-key", skip_serializing_if = "String::is_empty")]
    #[serde(deserialize_with = "null_default")]
    pub secret_access_key: String,
    #[serde(rename = "session-token", skip_serializing_if = "String::is_empty")]
    #[serde(deserialize_with = "null_default")]
    pub session_token: String,
}
/// Pure values from Go MeteringOSSConfig; no service or IO initialization.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MeteringOSSConfig {
    #[serde(rename = "assume-role-arn", skip_serializing_if = "String::is_empty")]
    #[serde(deserialize_with = "null_default")]
    pub assume_role_arn: String,
    #[serde(rename = "access-key", skip_serializing_if = "String::is_empty")]
    #[serde(deserialize_with = "null_default")]
    pub access_key: String,
    #[serde(rename = "secret-access-key", skip_serializing_if = "String::is_empty")]
    #[serde(deserialize_with = "null_default")]
    pub secret_access_key: String,
    #[serde(rename = "session-token", skip_serializing_if = "String::is_empty")]
    #[serde(deserialize_with = "null_default")]
    pub session_token: String,
}
/// Pure values from Go MeteringCOSConfig; no service or IO initialization.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MeteringCOSConfig {
    #[serde(rename = "assume-role-arn", skip_serializing_if = "String::is_empty")]
    #[serde(deserialize_with = "null_default")]
    pub assume_role_arn: String,
    #[serde(rename = "access-key", skip_serializing_if = "String::is_empty")]
    #[serde(deserialize_with = "null_default")]
    pub access_key: String,
    #[serde(rename = "secret-access-key", skip_serializing_if = "String::is_empty")]
    #[serde(deserialize_with = "null_default")]
    pub secret_access_key: String,
    #[serde(rename = "session-token", skip_serializing_if = "String::is_empty")]
    #[serde(deserialize_with = "null_default")]
    pub session_token: String,
}
/// Pure values from Go MeteringAzureConfig; no service or IO initialization.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MeteringAzureConfig {
    #[serde(rename = "account-name", skip_serializing_if = "String::is_empty")]
    #[serde(deserialize_with = "null_default")]
    pub account_name: String,
    #[serde(rename = "account-key", skip_serializing_if = "String::is_empty")]
    #[serde(deserialize_with = "null_default")]
    pub account_key: String,
    #[serde(rename = "sas-token", skip_serializing_if = "String::is_empty")]
    #[serde(deserialize_with = "null_default")]
    pub sas_token: String,
}
/// Pure values from Go MeteringLocalFSConfig; no service or IO initialization.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MeteringLocalFSConfig {
    #[serde(rename = "base-path", skip_serializing_if = "String::is_empty")]
    #[serde(deserialize_with = "null_default")]
    pub base_path: String,
    #[serde(rename = "create-dirs", skip_serializing_if = "is_false")]
    #[serde(deserialize_with = "null_default")]
    pub create_dirs: bool,
    #[serde(rename = "permissions", skip_serializing_if = "String::is_empty")]
    #[serde(deserialize_with = "null_default")]
    pub permissions: String,
}

fn is_false(v: &bool) -> bool {
    !v
}

fn null_default<'de, T: serde::Deserialize<'de> + Default, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<T, D::Error> {
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

#[cfg(test)]
mod config_test;
