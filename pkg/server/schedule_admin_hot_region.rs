// Copyright 2026 AsterSQL.
//! Relative hot-region configuration HTTP adapter; host owns mounting and locking.
use crate::schedule_admin_balance_leader::{json_type, syntax_error};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_schedulers::{HotConfigUpdate, HotRegionConfig, HotRegionParams};
use astersql_pd_storage_endpoint::StorageEndpoint;
use axum::{
    body::Body,
    http::{Method, Request},
    response::Response,
};
use futures_util::StreamExt;
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::Value;
fn response(status: u16, body: impl Into<String>, json: bool) -> Response {
    Response::builder()
        .status(status)
        .header(
            "content-type",
            if json {
                "application/json; charset=UTF-8"
            } else {
                "text/plain; charset=UTF-8"
            },
        )
        .body(Body::from(body.into()))
        .unwrap()
}
fn valid_duration<T: serde::de::DeserializeOwned>(_: &T, v: Value) -> bool {
    serde_json::from_value::<T>(v).is_ok()
}
fn error(status: u16, message: impl Into<String>) -> Response {
    response(
        status,
        serde_json::to_string(&message.into()).unwrap(),
        true,
    )
}
// Retain object order and duplicates for encoding/json's sequential field overlay.
struct Fields(Vec<(String, Value)>);
impl<'de> Deserialize<'de> for Fields {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Fields;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an object")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut m: M) -> Result<Fields, M::Error> {
                let mut v = vec![];
                while let Some(x) = m.next_entry()? {
                    v.push(x);
                }
                Ok(Fields(v))
            }
        }
        d.deserialize_map(V)
    }
}
/// Dispatch scheduler-relative paths with exclusive access spanning publication
/// and storage. Capability flags come from the host via HotRegionConfig::set_support.
pub async fn serve(
    config: &mut HotRegionConfig,
    storage: &StorageEndpoint,
    paths: &ClusterPaths,
    request: Request<Body>,
) -> Response {
    let path = request.uri().path();
    let method = match path {
        "/list" => Method::GET,
        "/config" => Method::POST,
        _ => {
            return Response::builder()
                .status(404)
                .header("content-type", "text/plain; charset=utf-8")
                .body(Body::from("404 page not found\n"))
                .unwrap();
        }
    };
    if request.method() != method {
        return Response::builder().status(405).body(Body::empty()).unwrap();
    }
    if path == "/list" {
        return response(
            200,
            serde_json::to_string_pretty(&config.valid_config()).unwrap(),
            true,
        );
    }
    let mut bytes = vec![];
    let mut stream = request.into_body().into_data_stream();
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(c) => bytes.extend_from_slice(&c),
            Err(e) => return error(500, e.to_string()),
        }
    }
    let value: Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => return error(500, syntax_error(&bytes, &e)),
    };
    if !value.is_object() && !value.is_null() {
        return error(
            500,
            format!(
                "json: cannot unmarshal {} into Go value of type schedulers.hotRegionSchedulerParam",
                json_type(&value)
            ),
        );
    }
    let fields = if value.is_null() {
        vec![]
    } else {
        serde_json::from_slice::<Fields>(&bytes).unwrap().0
    };
    let mut candidate = serde_json::to_value(config.params()).unwrap();
    candidate["forbid-rw-type"] = Value::String(config.params().forbid_rw_type.clone());
    let mut known = false;
    for (key, v) in fields {
        let Some(k) = candidate
            .as_object()
            .unwrap()
            .keys()
            .find(|k| k.eq_ignore_ascii_case(&key))
            .cloned()
        else {
            continue;
        };
        known |= key == k;
        // Go invokes Duration.UnmarshalJSON even for null; scalar null is otherwise ignored.
        if k.starts_with("history-sample-") {
            let Some(text) = v.as_str() else {
                return error(500, "invalid syntax");
            };
            if !valid_duration(&config.params().history_sample_duration, v.clone()) {
                return error(
                    500,
                    format!(
                        "time: invalid duration {}",
                        serde_json::to_string(text).unwrap()
                    ),
                );
            }
        } else if v.is_null() {
            if k.ends_with("priorities") {
                candidate[&k] = serde_json::json!([])
            }
            continue;
        }
        let old = &candidate[&k];
        let typ = if k.ends_with("priorities") {
            "[]string"
        } else if k == "max-peer-number" || k == "max-zombie-rounds" {
            "int"
        } else if old.is_number() {
            "float64"
        } else {
            "string"
        };
        if k == "strict-picking-store" || k == "enable-for-tiflash" {
            if !v.is_string() {
                return error(
                    500,
                    "json: invalid use of ,string struct tag, trying to unmarshal unquoted value into bool",
                );
            }
            if v == "null" {
                continue;
            }
            if v != "true" && v != "false" {
                return error(
                    500,
                    format!(
                        "json: invalid use of ,string struct tag, trying to unmarshal {} into bool",
                        serde_json::to_string(v.as_str().unwrap()).unwrap()
                    ),
                );
            }
        } else {
            let valid = match typ {
                "[]string" => v.is_array(),
                "int" => v.as_i64().is_some(),
                "float64" => v.is_number(),
                _ => v.is_string(),
            };
            if !valid {
                let kind = if v.is_number() {
                    format!("number {v}")
                } else {
                    json_type(&v).into()
                };
                return error(
                    500,
                    format!(
                        "json: cannot unmarshal {kind} into Go struct field hotRegionSchedulerParam.{k} of type {typ}"
                    ),
                );
            }
            if typ == "[]string" {
                let mut list = vec![];
                for entry in v.as_array().unwrap() {
                    if entry.is_null() {
                        list.push(Value::String(String::new()))
                    } else if entry.is_string() {
                        list.push(entry.clone())
                    } else {
                        return error(
                            500,
                            format!(
                                "json: cannot unmarshal {} into Go struct field hotRegionSchedulerParam.{k} of type string",
                                json_type(entry)
                            ),
                        );
                    }
                }
                candidate[&k] = Value::Array(list);
                continue;
            }
        }
        candidate[&k] = v;
    }
    let params: HotRegionParams = match serde_json::from_value(candidate) {
        Ok(p) => p,
        Err(e) => return error(500, e.to_string()),
    };
    if let Err(e) = params.validate() {
        return error(
            400,
            format!("[PD:scheduler:ErrSchedulerConfig]wrong scheduler config {e}"),
        );
    }
    if params == *config.params() && !known {
        return response(400, "Config item is not found.", false);
    }
    match config.update(params, storage, paths).await {
        Ok(HotConfigUpdate::Unchanged) => {
            response(200, "Config is the same with origin, so do nothing.", false)
        }
        // Go logs save failures but still acknowledges and retains the published value.
        Ok(HotConfigUpdate::Updated { .. }) => response(200, "Config is updated.", false),
        Err(e) => error(500, e.to_string()),
    }
}
