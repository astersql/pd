// Copyright 2026 AsterSQL.
//! Classic GET-with-JSON history adapter; domain storage owns selection/order.
use crate::{
    PdServices,
    http_contract::{go_json_strings, json_response},
    request_context,
};
use astersql_pd_storage::HistoryQuery;
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::State,
    http::{Method, StatusCode},
    response::Response,
    routing::any,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    Router::new()
        .route("/pd/api/v1/hotspot/regions/history", any(handle))
        .with_state((services, cancel))
}
type Raw = Box<serde_json::value::RawValue>;
fn kind(raw: &str) -> &'static str {
    match raw.as_bytes()[0] {
        b'[' => "array",
        b'{' => "object",
        b't' | b'f' => "bool",
        b'"' => "string",
        _ => "number",
    }
}
// Preserve source order, duplicate fields and the full uint64/int64 lexemes.
// Decoding through Value would sort case-folded duplicates and round overflow.
struct Fields(Vec<(String, Raw)>);
impl<'de> serde::Deserialize<'de> for Fields {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Fields;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a history request object")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Fields, A::Error> {
                let mut fields = Vec::new();
                while let Some(pair) = map.next_entry()? {
                    fields.push(pair);
                }
                Ok(Fields(fields))
            }
        }
        d.deserialize_map(Visitor)
    }
}
fn syntax_error(bytes: &[u8], error: &serde_json::Error) -> String {
    if error.is_eof() {
        return "unexpected end of JSON input".into();
    }
    let offset = bytes
        .split(|b| *b == b'\n')
        .take(error.line().saturating_sub(1))
        .map(|line| line.len() + 1)
        .sum::<usize>()
        + error.column().saturating_sub(1);
    let byte = bytes.get(offset).copied().unwrap_or(b'?');
    let quoted = match byte {
        b'\n' => "\\n".into(),
        b'\r' => "\\r".into(),
        b'\t' => "\\t".into(),
        b'\'' => "\\'".into(),
        b'\\' => "\\\\".into(),
        b if b.is_ascii() => (b as char).to_string(),
        b => format!("\\x{b:02x}"),
    };
    let reason = error.to_string();
    let suffix = if reason.starts_with("key must be a string") {
        "looking for beginning of object key string"
    } else if reason.starts_with("expected `:`") {
        "after object key"
    } else if reason.starts_with("expected `,` or `}`") {
        "after object key:value pair"
    } else if reason.starts_with("expected `,` or `]`") {
        "after array element"
    } else if reason.starts_with("trailing characters") {
        "after top-level value"
    } else if reason.starts_with("invalid escape") {
        "in string escape code"
    } else {
        "looking for beginning of value"
    };
    format!("invalid character '{quoted}' {suffix}")
}
fn parse(bytes: &[u8]) -> Result<HistoryQuery, String> {
    let clean = go_json_strings(bytes);
    let raw: Raw = serde_json::from_slice(&clean).map_err(|e| syntax_error(&clean, &e))?;
    let mut q = HistoryQuery::default();
    if raw.get() == "null" {
        return Ok(q);
    }
    if !raw.get().starts_with('{') {
        return Err(format!(
            "json: cannot unmarshal {} into Go value of type server.HistoryHotRegionsRequest",
            kind(raw.get())
        ));
    }
    let Fields(fields) =
        serde_json::from_str(raw.get()).map_err(|e: serde_json::Error| e.to_string())?;
    for (key, raw) in fields {
        let key = key
            .chars()
            .map(|c| {
                if c == 'ſ' {
                    's'
                } else {
                    c.to_ascii_lowercase()
                }
            })
            .collect::<String>();
        let v = raw.get();
        let typ = match key.as_str() {
            "start_time" | "end_time" => "int64",
            "region_ids" | "store_ids" | "peer_ids" => "[]uint64",
            "is_learners" | "is_leaders" => "[]bool",
            "hot_region_type" => "[]string",
            _ => continue,
        };
        if v == "null" {
            match key.as_str() {
                "region_ids" => q.region_ids.clear(),
                "store_ids" => q.store_ids.clear(),
                "peer_ids" => q.peer_ids.clear(),
                "is_learners" => q.is_learners.clear(),
                "is_leaders" => q.is_leaders.clear(),
                "hot_region_type" => q.hot_region_types.clear(),
                _ => {}
            }
            continue;
        }
        let error = |value: String, typ: &str| {
            format!(
                "json: cannot unmarshal {value} into Go struct field HistoryHotRegionsRequest.{key} of type {typ}"
            )
        };
        if typ == "int64" {
            let n = if kind(v) == "number" {
                v.parse::<i64>()
                    .map_err(|_| error(format!("number {v}"), typ))?
            } else {
                return Err(error(kind(v).into(), typ));
            };
            if key == "start_time" {
                q.start_time = n
            } else {
                q.end_time = n
            };
            continue;
        }
        if !v.starts_with('[') {
            return Err(error(kind(v).into(), typ));
        }
        let array: Vec<Raw> = serde_json::from_str(v).map_err(|e| e.to_string())?;
        match typ {
            "[]uint64" => {
                let mut out = Vec::with_capacity(array.len());
                for raw in array {
                    let v = raw.get();
                    out.push(if v == "null" {
                        0
                    } else if kind(v) == "number" {
                        v.parse::<u64>()
                            .map_err(|_| error(format!("number {v}"), "uint64"))?
                    } else {
                        return Err(error(kind(v).into(), "uint64"));
                    });
                }
                match key.as_str() {
                    "region_ids" => q.region_ids = out,
                    "store_ids" => q.store_ids = out,
                    _ => q.peer_ids = out,
                }
            }
            "[]bool" => {
                let mut out = Vec::with_capacity(array.len());
                for raw in array {
                    out.push(match raw.get() {
                        "null" | "false" => false,
                        "true" => true,
                        v => return Err(error(kind(v).into(), "bool")),
                    });
                }
                if key == "is_learners" {
                    q.is_learners = out
                } else {
                    q.is_leaders = out
                }
            }
            _ => {
                let mut out = Vec::with_capacity(array.len());
                for raw in array {
                    let v = raw.get();
                    out.push(if v == "null" {
                        String::new()
                    } else if v.starts_with('"') {
                        serde_json::from_str(v).map_err(|e| e.to_string())?
                    } else {
                        return Err(error(kind(v).into(), "string"));
                    });
                }
                q.hot_region_types = out;
            }
        }
    }
    Ok(q)
}
async fn handle(
    State((services, shutdown)): State<(Arc<dyn PdServices>, CancellationToken)>,
    method: Method,
    body: Body,
) -> Response {
    if method != Method::GET {
        return Response::builder()
            .status(if method == Method::OPTIONS {
                StatusCode::OK
            } else {
                StatusCode::METHOD_NOT_ALLOWED
            })
            .body(Body::empty())
            .unwrap();
    }
    let cancel = request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| shutdown.child_token());
    let work = async {
        let bytes = match to_bytes(body, usize::MAX).await {
            Ok(b) => b,
            Err(e) => return json_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        };
        let query = match parse(&bytes) {
            Ok(q) => q,
            Err(e) => return json_response(StatusCode::BAD_REQUEST, &e),
        };
        match services.hotspot_history(query).await {
            Ok(rows) => json_response(
                StatusCode::OK,
                &serde_json::json!({"history_hot_region":rows}),
            ),
            Err(e) => json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
        }
    };
    tokio::select! {biased;_=cancel.cancelled()=>json_response(StatusCode::INTERNAL_SERVER_ERROR,&"context canceled"),r=work=>r}
}
