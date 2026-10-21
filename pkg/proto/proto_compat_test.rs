// Copyright 2026 AsterSQL.

use prost::Message;

use crate::pdpb;

fn fixture<M: Message + Default>(name: &str) -> M {
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("wire-fixtures.json")).unwrap();
    let hex = fixtures[name].as_str().unwrap();
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    let message = M::decode(bytes.as_slice()).unwrap();
    assert_eq!(
        message.encode_to_vec(),
        bytes,
        "Go/Rust wire mismatch for {name}"
    );
    message
}

#[test]
fn proto_compat_request_header() {
    let header: pdpb::RequestHeader = fixture("request_header");
    assert_eq!((header.cluster_id, header.sender_id), (42, 7));
    assert_eq!(
        (header.caller_id.as_str(), header.caller_component.as_str()),
        ("oracle", "pd")
    );
}

#[test]
fn proto_compat_region_heartbeat() {
    let request: pdpb::RegionHeartbeatRequest = fixture("region_heartbeat");
    let region = request.region.unwrap();
    assert_eq!(region.id, 99);
    assert_eq!(region.start_key, [0, 1]);
    assert_eq!(region.end_key, [255]);
    assert_eq!(request.leader.unwrap().store_id, 4);
    assert_eq!(request.bytes_written, 1234);
}

#[test]
fn proto_compat_tso_request() {
    let request: pdpb::TsoRequest = fixture("tso_request");
    assert_eq!(request.count, 2);
    assert_eq!(request.dc_location, "global");
    assert_eq!(request.header.unwrap().cluster_id, 42);
}

#[test]
fn proto_compat_tso_response() {
    let response: pdpb::TsoResponse = fixture("tso_response");
    assert_eq!(response.count, 2);
    let timestamp = response.timestamp.unwrap();
    assert_eq!(
        (timestamp.physical, timestamp.logical, timestamp.suffix_bits),
        (123456, 17, 2)
    );
}

#[test]
fn proto_compat_member_response() {
    let response: pdpb::GetMembersResponse = fixture("members_response");
    assert_eq!(response.members.len(), 1);
    assert_eq!(response.members[0].client_urls, ["http://127.0.0.1:2379"]);
    assert_eq!(response.leader.unwrap().member_id, 9);
}

#[test]
fn proto_compat_oneof_preserves_explicit_zero() {
    let request: pdpb::GetGcSafePointV2Request = fixture("keyspace_oneof");
    assert_eq!(
        request.keyspace,
        Some(pdpb::get_gc_safe_point_v2_request::Keyspace::KeyspaceId(0))
    );
}

#[test]
fn proto_compat_error_enum() {
    let header: pdpb::ResponseHeader = fixture("error_enum");
    let error = header.error.unwrap();
    assert_eq!(error.r#type, pdpb::ErrorType::NotBootstrapped as i32);
    assert_eq!(error.r#type, 2);
    assert_eq!(error.message, "not bootstrapped");
}

#[test]
fn proto_compat_descriptors_and_streams() {
    let set = prost_types::FileDescriptorSet::decode(crate::FILE_DESCRIPTOR_SET).unwrap();
    for (package, service) in [
        ("pdpb", "PD"),
        ("keyspacepb", "Keyspace"),
        ("diagnosticspb", "Diagnostics"),
        ("resource_manager", "ResourceManager"),
        ("tsopb", "TSO"),
        ("schedulingpb", "Scheduling"),
        ("meta_storagepb", "MetaStorage"),
        ("routerpb", "Router"),
    ] {
        assert!(
            set.file
                .iter()
                .any(|file| file.package.as_deref() == Some(package)
                    && file
                        .service
                        .iter()
                        .any(|s| s.name.as_deref() == Some(service))),
            "missing {package}.{service}"
        );
    }
    let pd = set
        .file
        .iter()
        .find(|f| f.package.as_deref() == Some("pdpb"))
        .unwrap();
    let service = pd
        .service
        .iter()
        .find(|s| s.name.as_deref() == Some("PD"))
        .unwrap();
    for name in ["Tso", "RegionHeartbeat"] {
        let method = service
            .method
            .iter()
            .find(|m| m.name.as_deref() == Some(name))
            .unwrap();
        assert_eq!(
            (method.client_streaming, method.server_streaming),
            (Some(true), Some(true))
        );
    }
    let header = pd
        .message_type
        .iter()
        .find(|m| m.name.as_deref() == Some("RequestHeader"))
        .unwrap();
    assert_eq!(
        header
            .field
            .iter()
            .map(|f| (f.name.as_deref().unwrap(), f.number.unwrap()))
            .collect::<Vec<_>>(),
        [
            ("cluster_id", 1),
            ("sender_id", 2),
            ("caller_id", 3),
            ("caller_component", 4)
        ]
    );
}
