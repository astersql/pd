// Copyright 2026 AsterSQL.
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
pub(crate) struct ResponseGate {
    pub(crate) endpoint: String,
    pub(crate) armed: Arc<std::sync::atomic::AtomicBool>,
    pub(crate) held: Arc<tokio::sync::Notify>,
    stop: CancellationToken,
    arm_on: Arc<std::sync::Mutex<Option<Vec<u8>>>>,
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
        let arm_on = Arc::new(std::sync::Mutex::new(None::<Vec<u8>>));
        let trigger = arm_on.clone();
        let release = Arc::new(tokio::sync::Notify::new());
        let released = release.clone();
        let (gate, signal, shutdown) = (armed.clone(), held.clone(), stop.clone());
        let task = tokio::spawn(async move {
            let work = async {
                let (downstream, _) = listener.accept().await.unwrap();
                let upstream = tokio::net::TcpStream::connect(upstream).await.unwrap();
                let (mut dr, mut dw) = downstream.into_split();
                let (mut ur, mut uw) = upstream.into_split();
                let requests = async {
                    let mut buf = [0; 65536];
                    let mut recent = Vec::new();
                    loop {
                        let Ok(n) = dr.read(&mut buf).await else {
                            break;
                        };
                        if n == 0 {
                            break;
                        }
                        let needle = trigger.lock().unwrap().clone();
                        if let Some(needle) = needle {
                            recent.extend_from_slice(&buf[..n]);
                            if recent.windows(needle.len()).any(|part| part == needle) {
                                gate.store(true, Ordering::SeqCst);
                                trigger.lock().unwrap().take();
                            }
                            if recent.len() > needle.len() {
                                recent.drain(..recent.len() - needle.len());
                            }
                        }
                        if uw.write_all(&buf[..n]).await.is_err() {
                            break;
                        }
                    }
                };
                let responses = async {
                    let mut buf = [0; 65536];
                    loop {
                        let Ok(n) = ur.read(&mut buf).await else {
                            break;
                        };
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
                drop((dr, dw, ur, uw));
                std::future::pending::<()>().await;
            };
            tokio::select! { _ = shutdown.cancelled() => {}, _ = work => {} }
        });
        Self {
            endpoint,
            armed,
            held,
            stop,
            arm_on,
            release,
            task: Some(task),
        }
    }
    /// Arms at a real outbound key, so earlier read replies pass and the subsequent write reply is held.
    pub(crate) fn arm_on_key(&self, key: &[u8]) {
        assert!(!key.is_empty());
        *self.arm_on.lock().unwrap() = Some(key.to_vec());
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
