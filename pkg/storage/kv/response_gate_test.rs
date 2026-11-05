// Copyright 2026 AsterSQL.
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
pub(crate) struct ResponseGate {
    pub(crate) endpoint: String,
    pub(crate) armed: Arc<std::sync::atomic::AtomicBool>,
    pub(crate) held: Arc<tokio::sync::Notify>,
    stop: CancellationToken,
    release: Arc<tokio::sync::Notify>,
    task: Option<tokio::task::JoinHandle<()>>,
}
impl ResponseGate {
    pub(crate) async fn start(upstream: &str) -> Self {
        use std::sync::atomic::Ordering;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let upstream = upstream.strip_prefix("http://").unwrap().to_owned();
        let armed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let held = Arc::new(tokio::sync::Notify::new());
        let stop = CancellationToken::new();
        let release = Arc::new(tokio::sync::Notify::new());
        let released = release.clone();
        let (gate, signal, shutdown) = (armed.clone(), held.clone(), stop.clone());
        let task = tokio::spawn(async move {
            let work = async {
                let (downstream, _) = listener.accept().await.unwrap();
                let upstream = tokio::net::TcpStream::connect(upstream).await.unwrap();
                let (mut dr, mut dw) = downstream.into_split();
                let (mut ur, mut uw) = upstream.into_split();
                let requests = tokio::io::copy(&mut dr, &mut uw);
                let responses = async {
                    let mut buf = [0; 65536];
                    loop {
                        let n = ur.read(&mut buf).await.unwrap();
                        if n == 0 {
                            break;
                        }
                        if gate.load(Ordering::SeqCst) {
                            signal.notify_one();
                            released.notified().await;
                        }
                        if dw.write_all(&buf[..n]).await.is_err() {
                            break;
                        }
                    }
                };
                tokio::select! { _ = requests => {}, _ = responses => {} }
                std::future::pending::<()>().await;
            };
            tokio::select! { _ = shutdown.cancelled() => {}, _ = work => {} }
        });
        Self {
            endpoint,
            armed,
            held,
            stop,
            release,
            task: Some(task),
        }
    }
    pub(crate) fn release(&self) {
        self.armed.store(false, std::sync::atomic::Ordering::SeqCst);
        self.release.notify_one();
    }
    pub(crate) async fn close(mut self) {
        self.stop.cancel();
        self.task.take().unwrap().await.unwrap();
    }
}
impl Drop for ResponseGate {
    fn drop(&mut self) {
        self.stop.cancel();
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}
