// Copyright 2026 AsterSQL.
//! Strong transport ownership for native SDKs over the existing delegate kernel.
use super::DelegateConnections;
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll},
};
use tonic::Status;
use tower::ServiceExt;
/// Retains host pools while a native SDK owns its transport. Ordinary delegated
/// RPC clients remain weak. Endpoint fallback occurs only before a request is sent.
#[derive(Clone)]
pub struct OwnedDelegateChannel {
    targets: Arc<Vec<(String, Arc<DelegateConnections>)>>,
    next: Arc<AtomicUsize>,
}
impl OwnedDelegateChannel {
    pub fn new(targets: Vec<(String, Arc<DelegateConnections>)>) -> Result<Self, Status> {
        if targets.is_empty() {
            return Err(Status::invalid_argument("missing transport endpoints"));
        }
        Ok(Self {
            targets: Arc::new(targets),
            next: Arc::new(AtomicUsize::new(0)),
        })
    }
}
impl tower::Service<axum::http::Request<tonic::body::Body>> for OwnedDelegateChannel {
    type Response = axum::http::Response<tonic::body::Body>;
    type Error = Status;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;
    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Status>> {
        Poll::Ready(Ok(()))
    }
    fn call(&mut self, request: axum::http::Request<tonic::body::Body>) -> Self::Future {
        let targets = self.targets.clone();
        let first = self.next.fetch_add(1, Ordering::Relaxed) % targets.len();
        Box::pin(async move {
            let mut last = Status::unavailable("transport endpoints unavailable");
            for offset in 0..targets.len() {
                let (target, pool) = &targets[(first + offset) % targets.len()];
                match pool.get(target).await {
                    // No replay after call, including uncertain mutation outcomes.
                    Ok(channel) => return channel.oneshot(request).await,
                    Err(error) => last = error,
                }
            }
            Err(last)
        })
    }
}
