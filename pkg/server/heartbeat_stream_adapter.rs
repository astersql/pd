// Copyright 2026 AsterSQL.
//! Bounded transport I/O; cancellation also releases a router awaiting a slow peer.
use astersql_pd_schedule_hbstream::{
    HeartbeatCommand,
    routing::{HeartbeatStream, StreamError, StreamSend},
    stream_lifecycle::HeartbeatMessage,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
pub(crate) struct CommandStream {
    pub sender: mpsc::Sender<HeartbeatMessage>,
    pub cancellation: CancellationToken,
}
impl CommandStream {
    async fn send_message(&self, message: HeartbeatMessage) -> Result<(), StreamError> {
        tokio::select! { biased;
            _ = self.cancellation.cancelled() => Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe,"heartbeat stream closed").into()),
            result = tokio::time::timeout(std::time::Duration::from_secs(5), self.sender.send(message)) => {
                result.map_err(|e| Box::new(e) as StreamError)?
                    .map_err(|e| Box::new(e) as StreamError)
            }
        }
    }
}
impl HeartbeatStream for CommandStream {
    fn send(&self, command: HeartbeatCommand) -> StreamSend<'_> {
        Box::pin(self.send_message(HeartbeatMessage::Command(command)))
    }
    fn keep_alive(&self, cluster_id: u64) -> StreamSend<'_> {
        Box::pin(self.send_message(HeartbeatMessage::KeepAlive { cluster_id }))
    }
}
