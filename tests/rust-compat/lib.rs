// Copyright 2026 AsterSQL.
//! Real-process differential harness for the Go oracle and future Rust services.
pub mod harness;
pub mod normalize;

#[cfg(test)]
mod schedule_foundation;
#[cfg(test)]
#[path = "schedule_foundation_test.rs"]
mod schedule_foundation_test;

#[cfg(test)]
mod state_regression;
#[cfg(test)]
mod state_regression_test;

#[cfg(test)]
mod oracle_runner;
#[cfg(test)]
mod oracle_runner_test;
pub mod readonly_oracle;
#[cfg(test)]
mod readonly_oracle_test;

#[cfg(test)]
mod rolling_restart;
#[cfg(test)]
mod rolling_restart_test;

#[cfg(test)]
mod go_client_acceptance;
#[cfg(test)]
mod go_client_acceptance_test;

#[cfg(test)]
mod grpc_regression;
#[cfg(test)]
mod grpc_regression_test;

#[cfg(test)]
mod http_regression_test;

#[cfg(test)]
mod http_regression;

#[cfg(test)]
mod workspace_acceptance;
#[cfg(test)]
mod workspace_acceptance_test;

#[cfg(test)]
mod accept_dependency_test;

#[cfg(test)]
#[path = "meta_group_assignment_oracle_test.rs"]
mod meta_group_assignment_oracle_test;

#[cfg(test)]
mod group_merge_oracle_test;

#[cfg(test)]
mod group_nodes_oracle_test;

#[cfg(test)]
mod keyspace_split_oracle_test;

#[cfg(test)]
mod keyspace_create_oracle_test;

#[cfg(test)]
mod keyspace_config_oracle_test;

#[cfg(test)]
mod keyspace_state_oracle_test;

#[cfg(test)]
mod keyspace_remove_oracle_test;

#[cfg(test)]
mod keyspace_bootstrap_oracle_test;

#[cfg(test)]
mod group_remove_oracle_test;

#[cfg(test)]
mod gc_read_oracle_test;

#[cfg(test)]
mod txn_safe_point_oracle_test;

#[cfg(test)]
mod gc_barrier_oracle_test;

#[cfg(test)]
mod gc_global_barrier_oracle_test;

#[cfg(test)]
mod gc_safe_point_oracle_test;

#[cfg(test)]
mod accept_gc_03_test;

#[cfg(test)]
mod accept_gc_04_test;

#[cfg(test)]
mod accept_gc_05_test;

#[cfg(test)]
mod accept_gc_06_test;

#[cfg(test)]
mod accept_gc_07_test;

#[cfg(test)]
mod accept_gc_08_test;
#[cfg(test)]
mod accept_http_09_test;

#[cfg(test)]
mod accept_http_10_test;

#[cfg(test)]
mod accept_http_11_test;

#[cfg(test)]
mod accept_http_12_test;

#[cfg(test)]
mod accept_http_13_test;

#[cfg(test)]
mod accept_diagnostics_log_test;

#[cfg(test)]
mod accept_diagnostics_info_test;
#[cfg(test)]
mod accept_extra_metastorage_test;
#[cfg(test)]
mod accept_extra_rm_test;

#[cfg(test)]
mod accept_extra_rm_service_test;

#[cfg(test)]
mod accept_extra_router_service_test;

#[cfg(test)]
mod accept_extra_scheduling_service_test;

#[cfg(test)]
mod accept_extra_common_registration_test;

#[cfg(test)]
mod accept_extra_transport_config_test;

#[cfg(test)]
mod accept_extra_mcs_tls_test;

#[cfg(test)]
mod accept_extra_mcs_backend_tls_test;

#[cfg(test)]
mod accept_extra_mcs_process_config_test;

#[cfg(test)]
mod accept_extra_unregistered_test;

#[cfg(test)]
mod accept_extra_scheduling_operator_test;

#[cfg(test)]
mod accept_startup_config_test;
