// Copyright 2026 AsterSQL.
use crate::PdServices;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
pub fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> axum::Router {
    crate::http_member::router(services.clone(), cancel.clone())
        .merge(crate::http_leader::router(services.clone(), cancel.clone()))
        .merge(crate::http_cluster::router(
            services.clone(),
            cancel.clone(),
        ))
        .merge(crate::http_config_read::router(
            services.clone(),
            cancel.clone(),
        ))
        .merge(crate::http_replication_config_write::router(
            services.clone(),
            cancel.clone(),
        ))
        .merge(crate::http_schedule_config_write::router(services, cancel))
}
