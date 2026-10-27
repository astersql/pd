// Copyright 2026 AsterSQL.
//! Witness configuration HTTP dispatch; synchronization and mounting belong to the host.
use crate::schedule_admin_balance_leader::{decode_ranges, json_type, syntax_error};
use astersql_pd_core::KeyRange;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_schedulers::{BalanceWitnessConfig, BalanceWitnessScheduler};
use astersql_pd_storage_endpoint::StorageEndpoint;
use axum::{
    body::Body,
    http::{Method, Request},
    response::Response,
};
use futures_util::StreamExt;
use serde::{
    Deserialize, Deserializer,
    de::{MapAccess, Visitor},
};
use serde_json::Value;

fn response(status: u16, text: String, json: bool) -> Response {
    Response::builder()
        .status(status)
        .header(
            "content-type",
            if json {
                "application/json; charset=UTF-8"
            } else {
                "text/plain; charset=utf-8"
            },
        )
        .body(Body::from(text))
        .unwrap()
}
fn message(status: u16, text: &str) -> Response {
    response(status, serde_json::to_string_pretty(text).unwrap(), true)
}
// Keep top-level order and duplicate fields: Go decodes in input order, and a
// type error may leave other successfully decoded fields in the active config.
struct Fields(Vec<(String, Value)>);
impl<'de> Deserialize<'de> for Fields {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Object;
        impl<'de> Visitor<'de> for Object {
            type Value = Fields;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Fields, A::Error> {
                let mut fields = Vec::new();
                while let Some(field) = map.next_entry()? {
                    fields.push(field);
                }
                Ok(Fields(fields))
            }
        }
        d.deserialize_map(Object)
    }
}
/// Dispatch relative `/config` and `/list` paths against the real scheduler.
/// Exclusive borrowing spans mutation and storage I/O. Go's HTTP update publishes
/// before saving and returns success even when persistence fails; the validated
/// domain update API retains its separate rollback contract.
pub async fn serve(
    scheduler: &mut BalanceWitnessScheduler,
    storage: &StorageEndpoint,
    paths: &ClusterPaths,
    request: Request<Body>,
) -> Response {
    let path = request.uri().path();
    let expected = match path {
        "/config" => Method::POST,
        "/list" => Method::GET,
        _ => return response(404, "404 page not found\n".into(), false),
    };
    if request.method() != expected {
        return Response::builder().status(405).body(Body::empty()).unwrap();
    }
    if path == "/list" {
        let mut snapshot = serde_json::to_value(&scheduler.config).unwrap();
        // Go clone allocates a non-nil empty slice even when live ranges are nil.
        if snapshot["ranges"].is_null() {
            snapshot["ranges"] = serde_json::json!([]);
        }
        return response(200, serde_json::to_string_pretty(&snapshot).unwrap(), true);
    }
    let mut bytes = Vec::new();
    let mut stream = request.into_body().into_data_stream();
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(chunk) => bytes.extend_from_slice(&chunk),
            Err(_) => break,
        }
    }
    // Go validates JSON syntax before applying any fields.
    let patch: Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => return message(500, &syntax_error(&bytes, &e)),
    };
    if !patch.is_object() {
        return if patch.is_null() {
            message(400, "Config item is not found.")
        } else {
            message(
                500,
                &format!(
                    "json: cannot unmarshal {} into Go value of type schedulers.balanceWitnessSchedulerConfig",
                    json_type(&patch)
                ),
            )
        };
    }
    let Fields(fields) = serde_json::from_slice(&bytes).unwrap();
    let old = scheduler.config.clone();
    let mut ranges = old.transport_ranges().map(<[KeyRange]>::to_vec);
    let mut batch = old.transport_batch();
    let mut first_error = None;
    let mut known = false;
    for (key, value) in fields {
        known |= key == "batch" || key == "ranges";
        if key.eq_ignore_ascii_case("batch") && !value.is_null() {
            match value.as_i64() {
                Some(n) if value.as_number().unwrap().is_i64() => batch = n,
                _ => {
                    let kind = if value.is_number() {
                        format!("number {value}")
                    } else {
                        json_type(&value).into()
                    };
                    first_error.get_or_insert_with(|| format!("json: cannot unmarshal {kind} into Go struct field balanceWitnessSchedulerConfig.batch of type int"));
                }
            }
        } else if key.eq_ignore_ascii_case("ranges") {
            if value.is_null() {
                ranges = None;
            } else if let Some(items) = value.as_array() {
                // Go reuses existing slice elements. Custom KeyRange decode errors
                // stop immediately and retain already decoded elements and old tail.
                let active = ranges.get_or_insert_with(Vec::new);
                if items.is_empty() {
                    active.clear();
                }
                for (i, item) in items.iter().enumerate() {
                    if active.len() <= i {
                        active.push(KeyRange::default());
                    }
                    match decode_ranges(&Value::Array(vec![item.clone()])) {
                        Ok(mut decoded) => active[i] = decoded.remove(0),
                        Err(e) => {
                            scheduler.config =
                                BalanceWitnessConfig::from_transport_parts(ranges, batch);
                            return message(500, &e);
                        }
                    }
                }
                active.truncate(items.len());
            } else {
                first_error.get_or_insert_with(|| format!("json: cannot unmarshal {} into Go struct field balanceWitnessSchedulerConfig.ranges of type []keyutil.KeyRange", json_type(&value)));
            }
        }
    }
    scheduler.config = BalanceWitnessConfig::from_transport_parts(ranges, batch);
    if let Some(error) = first_error {
        return message(500, &error);
    }
    if serde_json::to_value(&old).unwrap() == serde_json::to_value(&scheduler.config).unwrap() {
        return if known {
            message(200, "Config is the same with origin, so do nothing.")
        } else {
            message(400, "Config item is not found.")
        };
    }
    if !(1..=10).contains(&batch) {
        scheduler.config = old;
        return message(
            400,
            "invalid batch size which should be an integer between 1 and 10",
        );
    }
    let encoded = scheduler
        .encode_witness_config()
        .expect("serializable configuration");
    // Persistence failure is deliberately non-fatal, as in Go update().
    let _persistence_error = storage
        .save_scheduler_config(paths, "balance-witness-scheduler", &encoded)
        .await
        .err();
    message(200, "Config is updated.")
}
