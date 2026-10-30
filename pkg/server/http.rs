// Copyright 2026 AsterSQL.
use crate::PdServices;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
pub fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> axum::Router {
    crate::http_member::router(services.clone(), cancel.clone())
        .merge(crate::http_scheduler_config::router(
            services.clone(),
            cancel.clone(),
        ))
        .merge(crate::http_scheduler::router(
            services.clone(),
            cancel.clone(),
        ))
        .merge(crate::http_operator_read::router(
            services.clone(),
            cancel.clone(),
        ))
        .merge(crate::http_region_scan::router(
            services.clone(),
            cancel.clone(),
        ))
        .merge(crate::http_region_lookup::router(
            services.clone(),
            cancel.clone(),
        ))
        .merge(crate::http_store_limit::router(
            services.clone(),
            cancel.clone(),
        ))
        .merge(crate::http_store_labels::router(
            services.clone(),
            cancel.clone(),
        ))
        .merge(crate::http_store_state::router(
            services.clone(),
            cancel.clone(),
        ))
        .merge(crate::http_store_read::router(
            services.clone(),
            cancel.clone(),
        ))
        .merge(crate::http_leader::router(services.clone(), cancel.clone()))
        .merge(crate::http_cluster::router(
            services.clone(),
            cancel.clone(),
        ))
        .merge(crate::http_config_read::router(
            services.clone(),
            cancel.clone(),
        ))
        .merge(crate::http_server_config_write::router(
            services.clone(),
            cancel.clone(),
        ))
        .merge(crate::http_replication_config_write::router(
            services.clone(),
            cancel.clone(),
        ))
        .merge(crate::http_hotspot_history::router(
            services.clone(),
            cancel.clone(),
        ))
        .merge(crate::http_schedule_config_write::router(services, cancel))
}
