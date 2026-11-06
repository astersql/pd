// Copyright 2026 AsterSQL.
use super::accept_extra_rm_service_test::Sidecar;
use crate::harness::Server;
use astersql_pd_proto::diagnosticspb::{self as d, diagnostics_client::DiagnosticsClient};
use prost::Message;
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};
use tonic::{Request, Response, Status, transport::Channel};
// Independently describe the pinned grpc-go health wire boundary in the oracle.
#[derive(Clone, PartialEq, Message)]
struct HealthRequest {
    #[prost(string, tag = "1")]
    service: String,
}
#[derive(Clone, PartialEq, Message)]
struct HealthResponse {
    #[prost(int32, tag = "1")]
    status: i32,
}
#[derive(Clone, PartialEq, Message)]
struct ListRequest {}
#[derive(Clone, PartialEq, Message)]
struct ListResponse {
    #[prost(btree_map = "string, message", tag = "1")]
    statuses: std::collections::BTreeMap<String, HealthResponse>,
}
fn error(error: Status) -> Value {
    json!({"code":format!("{:?}",error.code()),"message":error.message()})
}
async fn observe(channel: Channel) -> Value {
    let mut diagnostics = DiagnosticsClient::new(channel.clone());
    let info = match diagnostics
        .server_info(d::ServerInfoRequest { tp: 4 })
        .await
    {
        Ok(response) => json!({"protobuf":response.into_inner().encode_to_vec()}),
        Err(status) => error(status),
    };
    let logs = match diagnostics.search_log(d::SearchLogRequest::default()).await {
        Err(status) => error(status),
        Ok(response) => match response.into_inner().message().await {
            Err(status) => error(status),
            Ok(response) => json!({"unexpected":response.map(|r|r.encode_to_vec())}),
        },
    };
    let mut grpc = tonic::client::Grpc::new(channel);
    let mut checks = vec![];
    for service in ["", "missing"] {
        let request = HealthRequest {
            service: service.into(),
        };
        grpc.ready().await.unwrap();
        let response: Result<Response<HealthResponse>, Status> = grpc
            .unary(
                Request::new(request.clone()),
                tonic::codegen::http::uri::PathAndQuery::from_static(
                    "/grpc.health.v1.Health/Check",
                ),
                tonic_prost::ProstCodec::default(),
            )
            .await;
        checks.push(match response {
            Ok(r) => json!({"protobuf":r.into_inner().encode_to_vec()}),
            Err(status) => error(status),
        });
        grpc.ready().await.unwrap();
        let response: Result<Response<tonic::Streaming<HealthResponse>>, Status> = grpc
            .server_streaming(
                Request::new(request),
                tonic::codegen::http::uri::PathAndQuery::from_static(
                    "/grpc.health.v1.Health/Watch",
                ),
                tonic_prost::ProstCodec::default(),
            )
            .await;
        checks.push(match response {
            Err(status) => error(status),
            Ok(response) => match response.into_inner().message().await {
                Ok(Some(r)) => json!({"protobuf":r.encode_to_vec()}),
                Ok(None) => json!({"unexpected":"EOF"}),
                Err(status) => error(status),
            },
        });
    }
    grpc.ready().await.unwrap();
    let response: Result<Response<ListResponse>, Status> = grpc
        .unary(
            Request::new(ListRequest {}),
            tonic::codegen::http::uri::PathAndQuery::from_static("/grpc.health.v1.Health/List"),
            tonic_prost::ProstCodec::default(),
        )
        .await;
    let list = match response {
        Ok(r) => json!({"protobuf":r.into_inner().encode_to_vec()}),
        Err(status) => error(status),
    };
    json!({"info":info,"logs":logs,"checks":checks,"list":list})
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_all_mcs_share_diagnostics_and_health_registration() {
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut records = vec![];
    for (binary, label) in [(&go, "go"), (&rust, "rust")] {
        let mut pd = Server::start(&go).unwrap();
        pd.ready().unwrap();
        for service in ["resource-manager", "router", "scheduling", "tso"] {
            let mut sidecar = Sidecar::start_service(binary, pd.metadata_url(), service, "http");
            let channel = tokio::time::timeout(Duration::from_secs(15), async {
                loop {
                    match Channel::from_shared(sidecar.url.clone())
                        .unwrap()
                        .connect()
                        .await
                    {
                        Ok(channel) => break channel,
                        Err(_) => {
                            assert!(
                                sidecar.child.try_wait().unwrap().is_none(),
                                "{service}: {}",
                                sidecar.logs()
                            );
                            tokio::time::sleep(Duration::from_millis(20)).await;
                        }
                    }
                }
            })
            .await
            .unwrap();
            let result = tokio::time::timeout(Duration::from_secs(15), observe(channel))
                .await
                .unwrap();
            records.push(json!({"binary":label,"service":service,"result":result}));
            sidecar.stop();
        }
        pd.stop().unwrap();
    }
    let path = PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap())
        .with_extension("common-registration.json");
    std::fs::write(path, serde_json::to_vec(&records).unwrap()).unwrap();
    for n in 0..4 {
        let expected = &records[n]["result"];
        assert_eq!(expected["info"], json!({"protobuf":[]}));
        assert_eq!(
            expected["logs"],
            json!({"code":"Unknown","message":"empty log file location configuration"})
        );
        assert_eq!(
            expected["checks"],
            json!([{"protobuf":[8,1]},{"protobuf":[8,1]},{"code":"NotFound","message":"unknown service"},{"protobuf":[8,3]}])
        );
        assert_eq!(expected["list"], json!({"protobuf":[10,4,18,2,8,1]}));
        assert_eq!(
            records[n + 4]["result"],
            *expected,
            "{} registration mismatch",
            records[n]["service"]
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_all_mcs_diagnostics_reads_configured_host_log_file() {
    let work = super::accept_extra_transport_config_test::certificates("mcs-log");
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut records = vec![];
    for (binary, label) in [(&go, "go"), (&rust, "rust")] {
        let mut pd = Server::start(&go).unwrap();
        pd.ready().unwrap();
        for service in ["resource-manager", "router", "scheduling", "tso"] {
            let active = work.0.join(format!("{label}-{service}.log"));
            let fixture = work.0.join(format!("{label}-{service}-fixed.log"));
            std::fs::write(
                &fixture,
                "[2020/01/01 00:00:00.000 +00:00] [INFO] [mcs log probe]\n",
            )
            .unwrap();
            let arg = format!("--log-file={}", active.display());
            let mut sidecar = Sidecar::start_service_with_args(
                binary,
                pd.metadata_url(),
                service,
                "http",
                &[&arg],
            );
            let mut client=tokio::time::timeout(Duration::from_secs(15),async {
                loop {
                    match DiagnosticsClient::connect(sidecar.url.clone()).await {
                        Ok(client)=>break client,
                        Err(_)=>{
                            if let Some(status)=sidecar.child.try_wait().unwrap() {
                                records.push(json!({"binary":label,"service":service,"startup_failed":status.to_string(),"logs":sidecar.logs()}));
                                save_configured_log(&records);
                                panic!("{service} exited: {}",sidecar.logs());
                            }
                            tokio::time::sleep(Duration::from_millis(20)).await;
                        }
                    }
                }
            }).await.unwrap();
            assert!(
                active.is_file(),
                "logger must initialize the configured host file"
            );
            let mut stream = client
                .search_log(d::SearchLogRequest {
                    start_time: 1577836800000,
                    end_time: 1577836800001,
                    patterns: vec!["mcs log probe".into()],
                    ..Default::default()
                })
                .await
                .unwrap()
                .into_inner();
            let mut responses = vec![];
            let mut count = 0;
            while let Some(response) = stream.message().await.unwrap() {
                count += response.messages.len();
                responses.push(response.encode_to_vec());
            }
            assert_eq!(count, 1);
            records.push(json!({"binary":label,"service":service,"responses":responses,"configured_logger_created":true}));
            save_configured_log(&records);
            sidecar.stop();
        }
        pd.stop().unwrap();
    }
    save_configured_log(&records);
    for n in 0..4 {
        assert_eq!(records[n]["responses"], records[n + 4]["responses"]);
    }
}

fn save_configured_log(records: &[Value]) {
    let path = PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap())
        .with_extension("configured-mcs-log.json");
    std::fs::write(path, serde_json::to_vec(records).unwrap()).unwrap();
}
