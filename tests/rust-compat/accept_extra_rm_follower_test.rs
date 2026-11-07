// Copyright 2026 AsterSQL.
use super::{accept_extra_transport_config_test::Work, accept_modes::PdDeployment};
use astersql_pd_proto::{pdpb, resource_manager as p};
use astersql_pd_storage_kv::{Consistency, EtcdMetaStore, MetaStore, Read};
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};
fn group(priority: u32) -> p::ResourceGroup {
    p::ResourceGroup {
        name: "follower16".into(),
        mode: 1,
        priority,
        keyspace_id: Some(p::KeyspaceIdValue {
            keyspace: Some(p::keyspace_id_value::Keyspace::Value(7)),
        }),
        r_u_settings: Some(p::GroupRequestUnitSettings {
            r_u: Some(p::TokenBucket {
                settings: Some(p::TokenLimitSettings {
                    fill_rate: 500,
                    burst_limit: 2000,
                    max_tokens: 0.0,
                }),
                tokens: 25.0,
            }),
        }),
        ..Default::default()
    }
}
fn result<T: std::fmt::Debug>(
    r: Result<tonic::Response<T>, tonic::Status>,
    leader: &str,
    follower: &str,
) -> Value {
    match r {
        Ok(r) => json!({"response":format!("{:?}",r.into_inner())}),
        Err(e) => {
            json!({"code":format!("{:?}",e.code()),"message":e.message().replace(leader,"<leader>").replace(follower,"<follower>")})
        }
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_rm_real_follower_write_routing_and_fallback_roles() {
    matrix().await;
}
pub(super) async fn matrix() {
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let work = Work(std::env::temp_dir().join(format!("pd-rm-follower-{}", std::process::id())));
    std::fs::create_dir(&work.0).unwrap();
    let mut rows = vec![];
    for fallback in [true, false] {
        let config = work.0.join(format!("{fallback}.toml"));
        std::fs::write(
            &config,
            format!("[micro-service]\nenable-resource-manager-fallback={fallback}\n"),
        )
        .unwrap();
        let flag = format!("--config={}", config.display());
        for (binary, is_rust, embedded) in [
            (&go, false, false),
            (&rust, true, false),
            (&rust, true, true),
        ] {
            let env = if fallback {
                vec![]
            } else {
                vec![("PD_SERVICE_MODE".into(), "api".into())]
            };
            let mut leader =
                PdDeployment::start(binary, is_rust, embedded, fallback, &flag, &env, None).await;
            let mut follower = PdDeployment::start(
                binary,
                is_rust,
                embedded,
                fallback,
                &flag,
                &env,
                Some(&leader),
            )
            .await;
            let members = pdpb::pd_client::PdClient::connect(follower.url.clone())
                .await
                .unwrap()
                .get_members(pdpb::GetMembersRequest::default())
                .await
                .unwrap()
                .into_inner();
            assert_ne!(members.leader.as_ref().unwrap().name, "follower16");
            let store =
                EtcdMetaStore::connect(&[leader.metadata_url().into()], Duration::from_secs(2))
                    .await
                    .unwrap();
            let key = astersql_pd_keypath::keyspace_resource_group_setting_path(7, "follower16");
            let state_key =
                astersql_pd_keypath::keyspace_resource_group_state_path(7, "follower16");
            let mut client =
                p::resource_manager_client::ResourceManagerClient::connect(follower.url.clone())
                    .await
                    .unwrap();
            let before = store
                .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
                .await
                .unwrap();
            let added = result(
                client
                    .add_resource_group(p::PutResourceGroupRequest {
                        group: Some(group(8)),
                    })
                    .await,
                &leader.url,
                &follower.url,
            );
            let after_add = store
                .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
                .await
                .unwrap();
            let mut modify = tonic::Request::new(p::PutResourceGroupRequest {
                group: Some(group(12)),
            });
            modify
                .metadata_mut()
                .insert("pd-forwarded-host", leader.url.parse().unwrap());
            let modified = result(
                client.modify_resource_group(modify).await,
                &leader.url,
                &follower.url,
            );
            let after_modify = store
                .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
                .await
                .unwrap();
            let mut forged = tonic::Request::new(p::PutResourceGroupRequest {
                group: Some(group(14)),
            });
            forged
                .metadata_mut()
                .insert("pd-forwarded-host", follower.url.parse().unwrap());
            let rejected = result(
                client.modify_resource_group(forged).await,
                &leader.url,
                &follower.url,
            );
            let after_rejected = store
                .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
                .await
                .unwrap();
            assert_eq!(after_modify.entries, after_rejected.entries);
            let deleted = result(
                client
                    .delete_resource_group(p::DeleteResourceGroupRequest {
                        resource_group_name: "follower16".into(),
                        keyspace_id: group(0).keyspace_id,
                    })
                    .await,
                &leader.url,
                &follower.url,
            );
            let after_delete = store
                .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
                .await
                .unwrap();
            let states = store
                .read(Read::range(
                    state_key.as_bytes(),
                    b"",
                    Consistency::Linearizable,
                ))
                .await
                .unwrap();
            let encode = |entries: &Vec<astersql_pd_storage_kv::Entry>| {
                entries.iter().map(|e|json!({"key":e.key,"value":e.value,"create_revision":e.create_revision,"mod_revision":e.mod_revision,"version":e.version,"lease":e.lease})).collect::<Vec<_>>()
            };
            let row = json!({"fallback":fallback,"rust":is_rust,"embedded":embedded,"members":format!("{members:?}"),"added":added,"modified":modified,"rejected":rejected,"deleted":deleted,"before":encode(&before.entries),"after_add":encode(&after_add.entries),"after_modify":encode(&after_modify.entries),"after_rejected":encode(&after_rejected.entries),"after_delete":encode(&after_delete.entries),"states":encode(&states.entries)});
            rows.push(row);
            std::fs::write(
                PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap())
                    .with_extension("rm-follower.json"),
                serde_json::to_vec(&rows).unwrap(),
            )
            .unwrap();
            if fallback {
                assert_eq!(before.entries, after_add.entries);
                assert_eq!(before.entries, after_modify.entries);
                assert_eq!(before.entries, after_delete.entries);
            } else {
                assert_eq!(after_add.entries.len(), 1);
                assert_eq!(after_modify.entries.len(), 1);
                assert_ne!(after_add.entries[0].value, after_modify.entries[0].value);
                assert!(after_delete.entries.is_empty());
                assert!(
                    states.entries.is_empty(),
                    "PD metadata role must not persist RM token state"
                );
            }
            drop(client);
            drop(store);
            follower.stop().await;
            leader.stop().await;
        }
    }
    for pair in [[0, 1], [0, 2], [3, 4], [3, 5]] {
        for field in ["added", "modified", "rejected", "deleted"] {
            assert_eq!(rows[pair[0]][field], rows[pair[1]][field], "{field}");
        }
    }
}
