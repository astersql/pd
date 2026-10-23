// Copyright 2026 AsterSQL.
use crate::{EndpointError, StorageEndpoint};
use astersql_pd_keypath::{keyspace_service_limit_path, keyspace_service_limit_prefix};
use astersql_pd_storage_kv::Base;

impl StorageEndpoint {
    /// Load a limit, preserving Go's initial existence read and second JSON read.
    pub async fn load_service_limit(&self, id: u32) -> Result<f64, EndpointError> {
        let key = keyspace_service_limit_path(id);
        if Base::new(self.store.as_ref())
            .load(key.as_bytes())
            .await?
            .is_empty()
        {
            return Ok(0.0);
        }
        self.load_json(key.as_bytes()).await
    }
    /// Save one JSON float at its keyspace key.
    pub async fn save_service_limit(&self, id: u32, value: f64) -> Result<(), EndpointError> {
        let bytes = encode_limit(value)?;
        Ok(Base::new(self.store.as_ref())
            .save(keyspace_service_limit_path(id).as_bytes(), bytes.as_bytes())
            .await?)
    }
    /// Deliver valid records in physical key order, skipping malformed rows.
    pub async fn load_service_limits(
        &self,
        mut callback: impl FnMut(u32, f64),
    ) -> Result<(), EndpointError> {
        self.load_rule_prefix(keyspace_service_limit_prefix().as_bytes(), |key, value| {
            if key.is_empty() || !key.iter().all(u8::is_ascii_digit) {
                return;
            }
            let Ok(id) = std::str::from_utf8(key).unwrap().parse::<u32>() else {
                return;
            };
            if let Ok(value) = serde_json::from_slice::<Option<f64>>(value) {
                callback(id, value.unwrap_or_default());
            }
        })
        .await
    }
}

// Go encoding/json uses shortest decimal digits, fixed notation for [1e-6, 1e21),
// a signed positive exponent otherwise, and no redundant .0 for integral floats.
fn encode_limit(value: f64) -> Result<String, EndpointError> {
    if !value.is_finite() {
        return Err(EndpointError::JsonMarshal(
            <serde_json::Error as serde::ser::Error>::custom("unsupported non-finite float"),
        ));
    }
    let text = serde_json::to_string(&value).map_err(EndpointError::JsonMarshal)?;
    let Some((mantissa, exponent)) = text.split_once('e') else {
        return Ok(text.strip_suffix(".0").unwrap_or(&text).to_owned());
    };
    let exponent: i32 = exponent.parse().expect("JSON serializer exponent");
    let mantissa = mantissa.strip_suffix(".0").unwrap_or(mantissa);
    if value.abs() < 1e-6 || value.abs() >= 1e21 {
        return Ok(format!("{mantissa}e{exponent:+}"));
    }
    let negative = mantissa.starts_with('-');
    let unsigned = mantissa.strip_prefix('-').unwrap_or(mantissa);
    let point = unsigned.find('.').unwrap_or(unsigned.len()) as i32 + exponent;
    let digits = unsigned.replace('.', "");
    let decimal = if point <= 0 {
        format!("0.{}{digits}", "0".repeat((-point) as usize))
    } else if point as usize >= digits.len() {
        format!("{digits}{}", "0".repeat(point as usize - digits.len()))
    } else {
        format!(
            "{}.{}",
            &digits[..point as usize],
            &digits[point as usize..]
        )
    };
    Ok(if negative {
        format!("-{decimal}")
    } else {
        decimal
    })
}
