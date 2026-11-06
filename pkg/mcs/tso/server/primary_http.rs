// Copyright 2026 AsterSQL.
//! Node-local HTTP conversion over the same allocator watch as tonic.
use astersql_pd_mcs_discovery::{ServiceRegistryEntry, get_ms_members};
use astersql_pd_storage_domain::MetaStore;
use astersql_pd_tso::GroupAllocatorWatch;
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
#[derive(Clone)]
struct Context {
    watch: Arc<Mutex<GroupAllocatorWatch>>,
    store: Arc<dyn MetaStore>,
    cluster: u64,
    cancel: CancellationToken,
    entry: ServiceRegistryEntry,
}
pub(crate) fn router(
    watch: Arc<Mutex<GroupAllocatorWatch>>,
    store: Arc<dyn MetaStore>,
    cluster: u64,
    cancel: CancellationToken,
    entry: ServiceRegistryEntry,
) -> Router {
    Router::new()
        .route("/tso/api/v1/primary/evict", post(evict))
        .with_state(Context {
            watch,
            store,
            cluster,
            cancel,
            entry,
        })
}
fn json(status: u16, value: serde_json::Value) -> Response {
    (
        StatusCode::from_u16(status).unwrap(),
        [("content-type", "application/json; charset=utf-8")],
        serde_json::to_vec_pretty(&value).unwrap(),
    )
        .into_response()
}
fn text(status: u16, value: String) -> Response {
    (
        StatusCode::from_u16(status).unwrap(),
        [("content-type", "text/plain; charset=utf-8")],
        value,
    )
        .into_response()
}
async fn evict(State(c): State<Context>, body: Body) -> Response {
    let work = async {
        let bytes = match to_bytes(body, 4096).await {
            Ok(b) => b,
            Err(e) => {
                use std::error::Error;
                if e.source()
                    .is_some_and(|e| e.is::<http_body_util::LengthLimitError>())
                {
                    return text(413, "http: request body too large".into());
                }
                return text(400, "unexpected EOF".into());
            }
        };
        let mut target = String::new();
        if !bytes.is_empty() {
            let v: serde_json::Value = match serde_json::from_slice(&bytes) {
                Ok(v) => v,
                Err(e) => {
                    return text(
                        400,
                        if e.is_eof() {
                            "unexpected end of JSON input".into()
                        } else {
                            format!(
                                "invalid character '{}' looking for beginning of value",
                                bytes
                                    .get(e.column().saturating_sub(1))
                                    .copied()
                                    .unwrap_or(b'?') as char
                            )
                        },
                    );
                }
            };
            if !v.is_null() && !v.is_object() {
                return text(
                    400,
                    format!(
                        "json: cannot unmarshal {} into Go value of type apis.EvictPrimaryRequest",
                        typ(&v)
                    ),
                );
            }
            if v.is_object() {
                let fields: Fields = serde_json::from_slice(&bytes).expect("validated object");
                for (k, v) in &fields.0 {
                    if k.eq_ignore_ascii_case("new_primary") && !v.is_null() {
                        match v.as_str() {
                            Some(s) => target = s.into(),
                            None => {
                                return text(
                                    400,
                                    format!(
                                        "json: cannot unmarshal {} into Go struct field EvictPrimaryRequest.new_primary of type string",
                                        typ(v)
                                    ),
                                );
                            }
                        }
                    }
                }
            }
        }
        if !target.is_empty()
            && (target == c.entry.name
                || astersql_pd_typeutil::equal_base_urls(&target, &c.entry.service_addr))
        {
            return text(400, "new_primary must not be the node being evicted".into());
        }
        let entries = match get_ms_members(c.store.as_ref(), c.cluster, "tso", &c.cancel).await {
            Ok(e) => e.unwrap_or_default(),
            Err(e) => return json(500, serde_json::json!(e.to_string())),
        };
        match c
            .watch
            .lock()
            .await
            .evict_primaries(&entries, &target)
            .await
        {
            Ok(results) => json(
                if results.values().all(|v| v == "success") {
                    200
                } else {
                    500
                },
                serde_json::json!(results),
            ),
            Err((status, error)) => {
                if status == 400 {
                    text(status, error)
                } else {
                    json(status, serde_json::json!(error))
                }
            }
        }
    };
    tokio::select! {biased;_=c.cancel.cancelled()=>json(503,serde_json::json!("context canceled")),r=work=>r}
}

fn typ(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Null => "null",
    }
}

struct Fields(Vec<(String, serde_json::Value)>);
impl<'de> serde::Deserialize<'de> for Fields {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = Fields;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("object")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(self, mut m: M) -> Result<Fields, M::Error> {
                let mut fields = vec![];
                while let Some(f) = m.next_entry()? {
                    fields.push(f);
                }
                Ok(Fields(fields))
            }
        }
        d.deserialize_map(V)
    }
}
