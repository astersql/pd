// Copyright 2026 AsterSQL.
//! Deliberately incompatible HTTP service used only to exercise the harness.
use serde_json::json;
use std::{
    io::{Read, Write},
    net::TcpListener,
};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let value = |key: &str| {
        args.iter()
            .find_map(|a| a.strip_prefix(key))
            .unwrap()
            .to_owned()
    };
    let client = value("--client-urls=");
    let peer = value("--peer-urls=");
    let listener = TcpListener::bind(client.trim_start_matches("http://")).unwrap();
    for stream in listener.incoming() {
        let mut stream = stream.unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let mut bytes = [0; 4096];
        let n = stream.read(&mut bytes).unwrap();
        let request = String::from_utf8_lossy(&bytes[..n]);
        let member =
            json!({"name":"compat","member_id":42,"client_urls":[client],"peer_urls":[peer]});
        let body=if request.starts_with("GET /pd/api/v1/health ") {json!([{"name":"compat","member_id":42,"client_urls":[client],"health":!args.iter().any(|a|a=="--unhealthy")}])}
        else if request.starts_with("GET /pd/api/v1/members "){json!({"header":{"cluster_id":7},"members":[member.clone()],"leader":member})}
        else {json!({"schedule":{"max-snapshot-count":if args.iter().any(|a|a=="--semantic-difference"){65}else{64}},"replication":{"max-replicas":3}})}.to_string();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    }
}
