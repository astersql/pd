// Copyright 2026 AsterSQL.
//! Cancellation-safe, sticky startup acknowledgement for host readiness waits.
use tokio::sync::oneshot;

pub(crate) enum ReadyGate {
    Pending(oneshot::Receiver<()>),
    Ready,
    Failed,
}
impl ReadyGate {
    pub(crate) fn new(receiver: oneshot::Receiver<()>) -> Self {
        Self::Pending(receiver)
    }
    pub(crate) async fn wait(&mut self) -> Result<(), &'static str> {
        if let Self::Pending(receiver) = self {
            *self = if receiver.await.is_ok() {
                Self::Ready
            } else {
                Self::Failed
            };
        }
        match self {
            Self::Ready => Ok(()),
            _ => Err("startup acknowledgement closed"),
        }
    }
}
