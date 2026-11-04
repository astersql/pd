// Copyright 2026 AsterSQL.
//! TCP fixture that cancels an actual small etcd request before forwarding its body.
use std::{
    collections::HashMap,
    io,
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Mutex as AsyncMutex,
    task::{JoinHandle, JoinSet},
};
use tokio_util::sync::CancellationToken;

pub struct Proxy {
    pub address: SocketAddr,
    target: Arc<Mutex<Option<SocketAddr>>>,
    blocked_key: Arc<Mutex<Option<Vec<u8>>>>,
    rejected: Arc<Mutex<Vec<Vec<u8>>>>,
    cancel: CancellationToken,
    task: Option<JoinHandle<()>>,
}

impl Proxy {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let target = Arc::new(Mutex::new(None));
        let blocked_key = Arc::new(Mutex::new(None));
        let rejected = Arc::new(Mutex::new(Vec::new()));
        let cancel = CancellationToken::new();
        let inputs = (
            target.clone(),
            blocked_key.clone(),
            rejected.clone(),
            cancel.clone(),
        );
        let task = tokio::spawn(async move {
            let (target, key, rejected, cancel) = inputs;
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => break,
                    Some(_) = connections.join_next(), if !connections.is_empty() => {},
                    incoming = listener.accept() => {
                        let (stream, _) = incoming.unwrap();
                        let target = target.clone(); let key = key.clone(); let rejected = rejected.clone(); let cancel = cancel.clone();
                        connections.spawn(async move {
                            tokio::select! {
                                biased;
                                _ = cancel.cancelled() => {},
                                _ = connection(stream, target, key, rejected) => {},
                            }
                        });
                    }
                }
            }
            connections.abort_all();
            while connections.join_next().await.is_some() {}
        });
        Self {
            address,
            target,
            blocked_key,
            rejected,
            cancel,
            task: Some(task),
        }
    }
    pub fn target(&self, address: SocketAddr) {
        *self.target.lock().unwrap() = Some(address);
    }
    pub fn block(&self, key: &[u8]) {
        assert!(!key.is_empty());
        *self.blocked_key.lock().unwrap() = Some(key.to_vec());
    }
    pub fn unblock(&self) {
        *self.blocked_key.lock().unwrap() = None;
    }
    pub fn rejected(&self) -> Vec<Vec<u8>> {
        self.rejected.lock().unwrap().clone()
    }
    pub async fn stop(mut self) {
        self.cancel.cancel();
        self.task.take().unwrap().await.unwrap();
    }
}
impl Drop for Proxy {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

async fn frame<R: AsyncRead + Unpin>(reader: &mut R) -> io::Result<Vec<u8>> {
    let mut header = [0; 9];
    reader.read_exact(&mut header).await?;
    let length = ((header[0] as usize) << 16) | ((header[1] as usize) << 8) | header[2] as usize;
    if length > 65_536 {
        return Err(io::Error::other("fixture frame exceeds 64 KiB"));
    }
    let mut bytes = header.to_vec();
    bytes.resize(9 + length, 0);
    reader.read_exact(&mut bytes[9..]).await?;
    Ok(bytes)
}

async fn connection(
    mut client: TcpStream,
    target: Arc<Mutex<Option<SocketAddr>>>,
    blocked_key: Arc<Mutex<Option<Vec<u8>>>>,
    rejected: Arc<Mutex<Vec<Vec<u8>>>>,
) -> io::Result<()> {
    let address = loop {
        let address = *target.lock().unwrap();
        if let Some(address) = address {
            break address;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    };
    let mut upstream = TcpStream::connect(address).await?;
    let mut preface = [0; 24];
    client.read_exact(&mut preface).await?;
    upstream.write_all(&preface).await?;
    if &preface != b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n" {
        tokio::io::copy_bidirectional(&mut client, &mut upstream).await?;
        return Ok(());
    }
    let (mut client_read, client_write) = client.into_split();
    let (mut upstream_read, mut upstream_write) = upstream.into_split();
    let client_write = Arc::new(AsyncMutex::new(client_write));
    let destination = client_write.clone();
    let replies = async move {
        loop {
            // Whole frames prevent an injected RST_STREAM from splitting a server frame.
            let bytes = frame(&mut upstream_read).await?;
            destination.lock().await.write_all(&bytes).await?;
        }
        #[allow(unreachable_code)]
        Ok::<(), io::Error>(())
    };
    let requests = async move {
        let mut pending: HashMap<u32, (Vec<Vec<u8>>, Vec<u8>)> = HashMap::new();
        loop {
            let bytes = frame(&mut client_read).await?;
            if bytes[3] != 0 {
                upstream_write.write_all(&bytes).await?;
                continue;
            }
            let stream = u32::from_be_bytes(bytes[5..9].try_into().unwrap()) & 0x7fff_ffff;
            let (frames, body) = pending.entry(stream).or_default();
            let payload = &bytes[9..];
            if bytes[4] & 8 != 0 {
                let padding = *payload
                    .first()
                    .ok_or_else(|| io::Error::other("missing padding"))?
                    as usize;
                if padding + 1 > payload.len() {
                    return Err(io::Error::other("invalid padding"));
                }
                body.extend_from_slice(&payload[1..payload.len() - padding]);
            } else {
                body.extend_from_slice(payload);
            }
            frames.push(bytes);
            if body.len() < 5 {
                continue;
            }
            let length = u32::from_be_bytes(body[1..5].try_into().unwrap()) as usize;
            if length > 60_000 {
                return Err(io::Error::other(
                    "fixture request exceeds bounded flow-control window",
                ));
            }
            if body.len() < length + 5 {
                continue;
            }
            let reject = blocked_key
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|key| body.windows(key.len()).any(|part| part == key));
            if reject {
                // No DATA was forwarded, so the backend cannot commit this request.
                // RST_STREAM(CANCEL) affects this unary request, preserving lease streams.
                rejected.lock().unwrap().push(body.clone());
                let mut reset = vec![0, 0, 4, 3, 0];
                reset.extend_from_slice(&stream.to_be_bytes());
                reset.extend_from_slice(&8u32.to_be_bytes());
                upstream_write.write_all(&reset).await?;
                client_write.lock().await.write_all(&reset).await?;
            } else {
                for bytes in frames.iter() {
                    upstream_write.write_all(bytes).await?;
                }
            }
            pending.remove(&stream);
        }
        #[allow(unreachable_code)]
        Ok::<(), io::Error>(())
    };
    tokio::try_join!(requests, replies)?;
    Ok(())
}
