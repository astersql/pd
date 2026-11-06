// Copyright 2026 AsterSQL.
//! The classic host owns resource-group state and joins its periodic persistence worker.
use astersql_pd_member::Leadership;
use astersql_pd_rm_server::{Manager, WriteRole};
use astersql_pd_storage_domain::MetaStore;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
pub(crate) struct ResourceResources {
    pub(crate) manager: Arc<Mutex<Manager>>,
    cancel: CancellationToken,
    worker: Mutex<Option<tokio::task::JoinHandle<()>>>,
}
impl ResourceResources {
    pub(crate) async fn new(
        store: Arc<dyn MetaStore>,
        leader: Arc<Leadership>,
        parent: &CancellationToken,
        role: WriteRole,
        controller: astersql_pd_rm_server::ControllerConfig,
    ) -> Result<Self, String> {
        let cancel = parent.child_token();
        let store: Arc<dyn MetaStore> = Arc::new(crate::member_role_worker::TermStore::new(
            store,
            leader,
            cancel.clone(),
        ));
        let manager = Arc::new(Mutex::new(
            Manager::open_with_controller(store, role, controller).await?,
        ));
        let state = manager.clone();
        let stopped = cancel.clone();
        let worker = tokio::spawn(async move {
            let mut ticker = tokio::time::interval_at(
                tokio::time::Instant::now() + std::time::Duration::from_secs(60),
                std::time::Duration::from_secs(60),
            );
            loop {
                tokio::select! {biased;
                    _=stopped.cancelled()=>break,
                    _=ticker.tick()=>{
                        tokio::select!{biased;
                            _=stopped.cancelled()=>break,
                            _=async { if let Err(error)=state.lock().await.persist_states().await {
                                if !stopped.is_cancelled(){tracing::warn!(component="resource-manager", %error,"persist resource group states failed");}
                            } }=>{}
                        }
                    }
                }
            }
        });
        Ok(Self {
            manager,
            cancel,
            worker: Mutex::new(Some(worker)),
        })
    }
    pub(crate) async fn shutdown(&self) {
        self.cancel.cancel();
        if let Some(worker) = self.worker.lock().await.take() {
            if let Err(error) = worker.await {
                tracing::warn!(component="resource-manager", %error,"resource group worker failed");
            }
        }
    }
}
