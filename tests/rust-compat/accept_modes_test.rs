// Copyright 2026 AsterSQL.
//! The named148 target runs the same existing request sets through actual deployments.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_148_tso_modes() {
    super::accept_extra_tso_service_test::matrix().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_148_resource_manager_modes() {
    super::accept_extra_rm_domain_tls_test::matrix().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_148_classic_rm_fallback() {
    super::accept_extra_rm_follower_test::matrix().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_148_router_modes() {
    super::accept_extra_router_service_test::matrix().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_148_scheduling_split_modes() {
    super::accept_extra_scheduling_manual_test::scenario("split").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_148_scheduling_scatter_modes() {
    super::accept_extra_scheduling_manual_test::scenario("scatter").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_148_metastorage_modes() {
    super::accept_modes::metastorage_matrix().await;
}
