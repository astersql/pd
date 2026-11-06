// Copyright 2026 AsterSQL.
use super::DiscoveryError;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::{Compare, Consistency, MetaStore, Mutation, Read};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
/// Owns one address registration and its renewal worker. The host owns its runtime.
pub struct ServiceRegister {
    cancel: CancellationToken,
    worker: Option<tokio::task::JoinHandle<()>>,
    store: Arc<dyn MetaStore>,
    lease: Arc<tokio::sync::Mutex<Option<i64>>>,
}
impl Drop for ServiceRegister {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
async fn claim(
    store: &dyn MetaStore,
    key: &[u8],
    value: &[u8],
    prior: Option<i64>,
    ttl: i64,
) -> Result<i64, DiscoveryError> {
    let row = store
        .read(Read::range(key, b"", Consistency::Linearizable))
        .await?
        .entries
        .into_iter()
        .next();
    let cmp = if let Some(row) = row {
        if prior.is_none() || row.lease != prior {
            return Err(if row.lease.is_none() {
                DiscoveryError::Permanent
            } else {
                DiscoveryError::Occupied
            });
        }
        vec![Compare::Lease(key.to_vec(), prior.unwrap())]
    } else {
        vec![Compare::CreateRevision(key.to_vec(), 0)]
    };
    let lease = store.grant(ttl).await?;
    match store
        .transact(
            cmp,
            vec![Mutation::Put {
                key: key.to_vec(),
                value: value.to_vec(),
                lease: Some(lease.id),
            }],
        )
        .await
    {
        Ok(c) if c.succeeded => Ok(lease.id),
        result => {
            let _ = store.revoke(lease.id).await;
            match result {
                Err(e) => Err(e.into()),
                _ => Err(DiscoveryError::Occupied),
            }
        }
    }
}
impl ServiceRegister {
    /// Claim once atomically; duplicate live identities never overwrite the current owner.
    pub async fn start(
        store: Arc<dyn MetaStore>,
        runtime: tokio::runtime::Handle,
        cancel: CancellationToken,
        cluster: u64,
        service: &str,
        address: &str,
        value: Vec<u8>,
        ttl: i64,
    ) -> Result<Self, DiscoveryError> {
        let cancel = cancel.child_token();
        let key = ClusterPaths::new(cluster)
            .registry_path(service, address)
            .into_bytes();
        let id = tokio::select! {biased;_=cancel.cancelled()=>return Err(astersql_pd_storage_domain::StoreError::Cancelled.into()),r=claim(store.as_ref(),&key,&value,None,ttl)=>r?};
        let lease = Arc::new(tokio::sync::Mutex::new(Some(id)));
        let owned = lease.clone();
        let s = store.clone();
        let c = cancel.clone();
        let worker = runtime.spawn(async move {
            let mut tick =
                tokio::time::interval(Duration::from_millis((ttl.max(1) as u64) * 1000 / 3));
            loop {
                tokio::select! {biased;_=c.cancelled()=>break,_=tick.tick()=>{}}
                let prior = *owned.lock().await;
                let Some(id) = prior else { break };
                if s.keep_alive(id).await.is_ok() {
                    continue;
                }
                // A failed renewal never authorizes overwriting another instance.
                match claim(s.as_ref(), &key, &value, prior, ttl).await {
                    Ok(next) => {
                        *owned.lock().await = Some(next);
                        let _ = s.revoke(id).await;
                    }
                    Err(DiscoveryError::Permanent) => break,
                    Err(_) => {}
                }
            }
        });
        Ok(Self {
            cancel,
            worker: Some(worker),
            store,
            lease,
        })
    }
    /// Join renewal before revoking this instance's own lease. Repeated cancellation is safe.
    pub async fn shutdown(mut self) -> Result<(), DiscoveryError> {
        self.cancel.cancel();
        if let Some(w) = self.worker.take() {
            let _ = w.await;
        }
        if let Some(id) = self.lease.lock().await.take() {
            match self.store.revoke(id).await {
                Ok(()) | Err(astersql_pd_storage_domain::StoreError::LeaseNotFound) => {}
                Err(e) => return Err(e.into()),
            };
        }
        Ok(())
    }
}
