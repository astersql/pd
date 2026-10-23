// Copyright 2026 AsterSQL.
//! TLS configuration values; certificate IO remains host-owned.
use serde::{Deserialize, Serialize};
/// Pure values from Go TLSConfig; no service or IO initialization.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TLSConfig {
    #[serde(rename = "cacert-path")]
    #[serde(deserialize_with = "null_default")]
    pub ca_path: String,
    #[serde(rename = "cert-path")]
    #[serde(deserialize_with = "null_default")]
    pub cert_path: String,
    #[serde(rename = "key-path")]
    #[serde(deserialize_with = "null_default")]
    pub key_path: String,
    #[serde(rename = "cert-allowed-cn")]
    pub cert_allowed_c_ns: Option<Vec<String>>,
    #[serde(rename = "SSLCABytes", with = "byte_slice")]
    pub sslca_bytes: Option<Vec<u8>>,
    #[serde(rename = "SSLCertBytes", with = "byte_slice")]
    pub ssl_cert_bytes: Option<Vec<u8>>,
    #[serde(rename = "SSLKEYBytes", with = "byte_slice")]
    pub sslkey_bytes: Option<Vec<u8>>,
}

mod byte_slice {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(v: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            None => s.serialize_none(),
            Some(v) => s.serialize_str(&STANDARD.encode(v)),
        }
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
        let v = Option::<serde_json::Value>::deserialize(d)?;
        match v {
            None => Ok(None),
            Some(serde_json::Value::String(s)) => STANDARD
                .decode(s.replace(['\r', '\n'], ""))
                .map(Some)
                .map_err(serde::de::Error::custom),
            Some(v @ serde_json::Value::Array(_)) => serde_json::from_value(v)
                .map(Some)
                .map_err(serde::de::Error::custom),
            _ => Err(serde::de::Error::custom(
                "expected base64 string or byte array",
            )),
        }
    }
}

fn null_default<'de, T: serde::Deserialize<'de> + Default, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<T, D::Error> {
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

#[cfg(test)]
mod config_test;
