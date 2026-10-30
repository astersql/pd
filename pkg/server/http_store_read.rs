// Copyright 2026 AsterSQL.
//! Store HTTP projections over the host-owned cluster view.
use crate::{
    PdServices,
    http_contract::{json_response, parse_uint64_field, query_unescape},
    request_context,
};
use astersql_pd_core::{StoreInfo, StoreState};
use astersql_pd_typeutil::{ByteSize, Duration};
use axum::{
    Router,
    extract::{OriginalUri, State},
    http::{StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct Stores {
    services: Arc<dyn PdServices>,
    cancel: CancellationToken,
}
fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}
fn timestamp(nanos: i64) -> String {
    chrono::DateTime::from_timestamp(
        nanos.div_euclid(1_000_000_000),
        nanos.rem_euclid(1_000_000_000) as u32,
    )
    .unwrap()
    .to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
}
pub(crate) fn project(s: &StoreInfo, cfg: &astersql_pd_server_config::Config, now: i128) -> Value {
    let m = crate::core_store_adapter::store_to_proto(s.meta().cloned().unwrap_or_default());
    let mut meta = json!({});
    macro_rules! nonzero { ($($key:ident),*) => { $(if !is_default(&m.$key) { meta[stringify!($key)] = json!(m.$key); })* }; }
    nonzero!(
        id,
        state,
        node_state,
        start_timestamp,
        last_heartbeat,
        physically_destroyed
    );
    macro_rules! text { ($($key:ident),*) => { $(if !m.$key.is_empty() { meta[stringify!($key)] = json!(m.$key); })* }; }
    text!(
        address,
        peer_address,
        status_address,
        version,
        git_hash,
        deploy_path
    );
    if !m.labels.is_empty() {
        meta["labels"] = json!(
            m.labels
                .iter()
                .map(|l| {
                    let mut v = json!({});
                    if !l.key.is_empty() {
                        v["key"] = json!(l.key);
                    }
                    if !l.value.is_empty() {
                        v["value"] = json!(l.value);
                    }
                    v
                })
                .collect::<Vec<_>>()
        );
    }
    if let Some(r) = m.txn_protocol_version_range {
        let mut v = json!({});
        if r.min != 0 {
            v["min"] = json!(r.min);
        }
        if r.max != 0 {
            v["max"] = json!(r.max);
        }
        meta["txn_protocol_version_range"] = v;
    }
    let opt = &cfg.schedule;
    meta["state_name"] = json!(match s.state() {
        StoreState::Up if s.down_time_nanos(now) > opt.max_store_down_time.as_nanos() =>
            "Down".into(),
        StoreState::Up if s.is_disconnected(now) => "Disconnected".into(),
        StoreState::Up => "Up".into(),
        StoreState::Offline => "Offline".into(),
        StoreState::Tombstone => "Tombstone".into(),
        StoreState::Unknown(n) => n.to_string(),
    });
    let mut status = json!({"capacity":ByteSize::from_bytes(s.capacity()),"available":ByteSize::from_bytes(s.available()),"used_size":ByteSize::from_bytes(s.used_size()),"leader_count":s.leader_count(),"leader_weight":s.leader_weight(),"leader_score":s.leader_score(astersql_pd_core_constant::string_to_schedule_policy(&opt.leader_schedule_policy),0),"leader_size":s.leader_size(),"region_count":s.region_count(),"region_weight":s.region_weight(),"region_score":s.region_score(&opt.region_score_formula_version,opt.high_space_ratio,opt.low_space_ratio,0),"region_size":s.region_size()});
    macro_rules! count { ($($key:ident),*) => { $(if s.$key()!=0 {status[stringify!($key)]=json!(s.$key());})* }; }
    count!(
        learner_count,
        witness_count,
        pending_peer_count,
        slow_score,
        sending_snap_count,
        receiving_snap_count
    );
    if s.is_busy() {
        status["is_busy"] = json!(true);
    }
    if !s.allow_leader_transfer_in() {
        status["pause_leader_transfer_in"] = json!(true);
    }
    if !s.allow_leader_transfer_out() {
        status["pause_leader_transfer_out"] = json!(true);
    }
    let stats = s.store_stats();
    if !stats.network_slow_scores.is_empty() {
        status["network_slow_score"] = json!(stats.network_slow_scores);
    }
    if let Some(t) = stats.slow_trend {
        status["slow_trend"] = json!({"cause_value":t.cause_value,"cause_rate":t.cause_rate,"result_value":t.result_value,"result_rate":t.result_rate});
    }
    status["start_ts"] = json!(
        chrono::DateTime::from_timestamp(s.start_time_unix_seconds(), 0)
            .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
    );
    status["last_heartbeat_ts"] = json!(timestamp(s.last_heartbeat_unix_nanos()));
    if s.uptime_nanos() > 0 {
        status["uptime"] = json!(Duration::from_nanos(s.uptime_nanos()));
    }
    json!({"store":meta,"status":status})
}
fn query(uri: &Uri) -> Vec<(String, String)> {
    uri.query()
        .unwrap_or_default()
        .split('&')
        .filter(|p| !p.contains(';'))
        .filter_map(|pair| {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            Some((
                String::from_utf8_lossy(&query_unescape(k).ok()?).into_owned(),
                String::from_utf8_lossy(&query_unescape(v).ok()?).into_owned(),
            ))
        })
        .collect()
}
async fn read(State(s): State<Stores>, OriginalUri(uri): OriginalUri) -> Response {
    let token = request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| s.cancel.child_token());
    let work = async {
        let bootstrapped = match s.services.is_bootstrapped().await {
            Ok(value) => value,
            Err(e) => return json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
        };
        if !bootstrapped {
            return json_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &"[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first",
            );
        }
        let result = async {
            let cfg = s.services.configuration(false).await?;
            let view = s
                .services
                .cluster_read_service()
                .ok_or("cluster reads are not configured")?;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as i128;
            if let Some(id) = uri.path().strip_prefix("/pd/api/v1/store/") {
                let decoded = query_unescape(&id.replace('+', "%2B"))
                    .unwrap_or_else(|_| id.as_bytes().to_vec());
                let id = String::from_utf8_lossy(&decoded);
                let id = match parse_uint64_field(Some(&id), "id") {
                    Ok(n) => n,
                    Err(crate::http_contract::HttpError::Input(message)) => {
                        let mut response = json_response(
                            StatusCode::BAD_REQUEST,
                            &json!({"code":"input","msg":message,"data":{}}),
                        );
                        response
                            .headers_mut()
                            .insert("tidb-error-code", "input".parse().unwrap());
                        return Ok(response);
                    }
                    Err(e) => return Ok(e.into_response()),
                };
                return Ok(match view.store(id).await? {
                    Some(store) => json_response(StatusCode::OK, &project(&store, &cfg, now)),
                    None => json_response(
                        StatusCode::NOT_FOUND,
                        &format!("[PD:core:ErrStoreNotFound]store {id} not found"),
                    ),
                });
            }
            let q = query(&uri);
            let states: Vec<_> = q
                .iter()
                .filter(|(k, _)| k == "state")
                .map(|(_, v)| v.as_str())
                .collect();
            let check = uri.path().ends_with("/check");
            let labels = uri.path().ends_with("/labels/stores");
            let mut accepted = vec![];
            if !labels && !check {
                for value in &states {
                    let state = match value.parse::<i64>() {
                        Ok(n) if !value.is_empty() && value.trim() == *value => n,
                        _ => {
                            return Err(format!(
                                "strconv.Atoi: parsing {}: {}",
                                json!(value),
                                if {
                                    let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
                                    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
                                } {
                                    "value out of range"
                                } else {
                                    "invalid syntax"
                                }
                            ));
                        }
                    };
                    if !(0..=2).contains(&(state as i32)) {
                        return Err(format!("unknown StoreState: {}", state as i32));
                    }
                    accepted.push(state as i32);
                }
                if states.is_empty() {
                    accepted = vec![0, 1];
                }
            }
            if check {
                for v in &states {
                    if !v.is_empty()
                        && !["up", "offline", "tombstone", "down", "disconnected"]
                            .contains(&v.to_lowercase().as_str())
                    {
                        return Ok(json_response(
                            StatusCode::BAD_REQUEST,
                            &format!("unknown StoreState: {v}"),
                        ));
                    }
                }
            }
            let patterns = if labels {
                let get = |key| {
                    q.iter()
                        .find(|(k, _)| k == key)
                        .map(|(_, v)| v.as_str())
                        .unwrap_or("")
                };
                Some((label_pattern(get("name"))?, label_pattern(get("value"))?))
            } else {
                None
            };
            let mut stores = vec![];
            for m in s.services.stores().await? {
                let state = match m.state {
                    StoreState::Up => 0,
                    StoreState::Offline => 1,
                    StoreState::Tombstone => 2,
                    StoreState::Unknown(n) => n,
                };
                if !check && !labels && !accepted.contains(&state) {
                    continue;
                }
                if let Some((k, v)) = &patterns {
                    if !m
                        .labels
                        .iter()
                        .any(|l| k.is_match(&l.key) && v.is_match(&l.value))
                    {
                        continue;
                    }
                }
                let store = view
                    .store(m.id)
                    .await?
                    .ok_or_else(|| format!("[PD:core:ErrStoreNotFound]store {} not found", m.id))?;
                let item = project(&store, &cfg, now);
                if check
                    && states.iter().any(|v| !v.is_empty())
                    && !states.iter().any(|v| {
                        v.eq_ignore_ascii_case(item["store"]["state_name"].as_str().unwrap())
                    })
                {
                    continue;
                }
                stores.push(item);
            }
            Ok::<_, String>(json_response(
                StatusCode::OK,
                &json!({"count":stores.len(),"stores":stores}),
            ))
        }
        .await;
        result.unwrap_or_else(|e| json_response(StatusCode::INTERNAL_SERVER_ERROR, &e))
    };
    tokio::select! {biased; _=token.cancelled()=>json_response(StatusCode::SERVICE_UNAVAILABLE,&"server not started"),r=work=>r}
}
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    Router::new()
        .route("/pd/api/v1/store/{id}", get(read))
        .route("/pd/api/v1/stores", get(read))
        .route("/pd/api/v1/stores/check", get(read))
        .route("/pd/api/v1/labels/stores", get(read))
        .with_state(Stores { services, cancel })
}

fn label_pattern(input: &str) -> Result<regex::Regex, String> {
    go_pattern(&format!("(?i){input}"))
}
/// Shared RE2-compatible parsing for HTTP label and placement selectors.
pub(crate) fn go_pattern(pattern: &str) -> Result<regex::Regex, String> {
    // Go's Perl classes and word boundaries are ASCII, while its literal
    // case folding remains Unicode. Quoted literals are accepted by RE2.
    let mut translated = String::new();
    let mut chars = pattern.chars().peekable();
    let mut in_class = false;
    let mut class_first = false;
    while let Some(c) = chars.next() {
        if c == '[' {
            if in_class && chars.peek() != Some(&':') {
                translated.push_str("\\[");
                class_first = false;
                continue;
            }
            if in_class {
                // POSIX classes retain their own closing bracket.
                let mut posix = String::from("[");
                while let Some(c) = chars.next() {
                    posix.push(c);
                    if c == ']' {
                        break;
                    }
                }
                translated.push_str(&posix);
                class_first = false;
                continue;
            }
            in_class = true;
            class_first = true;
            translated.push(c);
            continue;
        }
        if in_class && c != '\\' {
            if c == ']' && !class_first {
                in_class = false;
                translated.push(c);
                continue;
            }
            if c == '^' && class_first {
                translated.push(c);
                continue;
            }
            class_first = false;
            // RE2 has no Rust character-class intersection or subtraction.
            if matches!(c, '&' | '~' | '[' | ']') {
                translated.push('\\');
            }
            translated.push(c);
            continue;
        }
        if c == '{' && !in_class {
            let tail: String = chars.clone().take_while(|c| *c != '}').collect();
            let parts: Vec<_> = tail.split(',').collect();
            let valid = chars.clone().nth(tail.chars().count()) == Some('}')
                && parts.len() <= 2
                && !parts[0].is_empty()
                && parts.iter().all(|p| {
                    p.bytes().all(|b| b.is_ascii_digit()) && !(p.len() > 1 && p.starts_with('0'))
                });
            if valid {
                let bounds: Vec<_> = parts
                    .iter()
                    .filter(|p| !p.is_empty())
                    .map(|p| p.parse::<u64>().unwrap_or(u64::MAX))
                    .collect();
                if bounds.iter().any(|n| *n > 1000) || bounds.len() == 2 && bounds[0] > bounds[1] {
                    return Err(format!(
                        "error parsing regexp: invalid repeat count: `{{{tail}}}`"
                    ));
                }
                translated.push_str(&format!("{{{tail}}}"));
                for _ in 0..=tail.chars().count() {
                    chars.next();
                }
            } else {
                translated.push_str("\\{");
            }
            continue;
        }
        if c != '\\' {
            translated.push(c);
            continue;
        }
        let Some(c) = chars.next() else {
            return Err("error parsing regexp: trailing backslash at end of expression: ``".into());
        };
        if in_class {
            class_first = false;
        }
        if ('0'..='7').contains(&c) {
            if c != '0' && !chars.peek().is_some_and(|c| ('0'..='7').contains(c)) {
                return Err(format!(
                    "error parsing regexp: invalid escape sequence: `\\{c}`"
                ));
            }
            let mut value = c as u32 - '0' as u32;
            for _ in 0..2 {
                if let Some(c) = chars.peek().filter(|c| ('0'..='7').contains(c)).copied() {
                    chars.next();
                    value = value * 8 + c as u32 - '0' as u32;
                } else {
                    break;
                }
            }
            translated.push_str(&format!("\\x{{{value:x}}}"));
            continue;
        }
        if matches!(c, 'u' | 'U') {
            return Err(format!(
                "error parsing regexp: invalid escape sequence: `\\{c}`"
            ));
        }
        if c.is_ascii() && !c.is_ascii_alphanumeric() {
            translated.push_str(&regex::escape(&c.to_string()));
            continue;
        }

        if c == 'Q' {
            let mut literal = String::new();
            while let Some(c) = chars.next() {
                if c == '\\' && chars.peek() == Some(&'E') {
                    chars.next();
                    break;
                }
                literal.push(c);
            }
            translated.push_str(&regex::escape(&literal));
            continue;
        }
        match c {
            'd' => translated.push_str("[0-9]"),
            'D' => translated.push_str("[^0-9]"),
            'w' => translated.push_str("[A-Za-z0-9_]"),
            'W' => translated.push_str("[^A-Za-z0-9_]"),
            's' => translated.push_str("[\\t\\n\\f\\r ]"),
            'S' => translated.push_str("[^\\t\\n\\f\\r ]"),
            'b' => translated.push_str("(?-u:\\b)"),
            'B' => translated.push_str("(?-u:\\B)"),
            _ => {
                translated.push('\\');
                translated.push(c);
            }
        }
    }
    regex::Regex::new(&translated).map_err(|e| {
        let text = e.to_string();
        let lines: Vec<_> = text.lines().collect();
        let marker = lines.get(2).copied().unwrap_or("");
        let start = marker.find('^').unwrap_or(4).saturating_sub(4);
        let width = marker.bytes().filter(|b| *b == b'^').count();
        let span: String = translated.chars().skip(start).take(width).collect();
        let error = |kind: &str, expr: &str| format!("error parsing regexp: {kind}: `{expr}`");
        if text.contains("missing expression") {
            error("missing argument to repetition operator", &span)
        } else if text.contains("unrecognized escape sequence")
            || text.contains("backreferences are not supported")
        {
            error("invalid escape sequence", &span)
        } else if text.contains("look-around") || text.contains("unrecognized flag") {
            error("invalid or unsupported Perl syntax", &span)
        } else if text.contains("invalid character class range") {
            error("invalid character class range", &span)
        } else if text.contains("unclosed character class") {
            format!(
                "error parsing regexp: missing closing ]: `{}`",
                &pattern[pattern.rfind('[').unwrap_or(0)..]
            )
        } else if text.contains("unclosed group") {
            format!("error parsing regexp: missing closing ): `{pattern}`")
        } else if text.contains("unopened group") {
            format!("error parsing regexp: unexpected ): `{pattern}`")
        } else {
            text
        }
    })
}
