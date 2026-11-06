// Copyright 2026 AsterSQL.
use std::{
    future::Future,
    io,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_util::sync::CancellationToken;
/// Close transport IO even if a caller retains a cloned channel or response body.
pub(crate) struct CancellableIo<T> {
    io: T,
    cancelled: Pin<Box<dyn Future<Output = ()> + Send>>,
}
impl<T> CancellableIo<T> {
    pub fn new(io: T, cancel: CancellationToken) -> Self {
        Self {
            io,
            cancelled: Box::pin(cancel.cancelled_owned()),
        }
    }
    fn closed(&mut self, cx: &mut Context<'_>) -> bool {
        self.cancelled.as_mut().poll(cx).is_ready()
    }
}
fn closed() -> io::Error {
    io::Error::new(
        io::ErrorKind::ConnectionAborted,
        "microservice connection closed",
    )
}
impl<T: AsyncRead + Unpin> AsyncRead for CancellableIo<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if this.closed(cx) {
            return Poll::Ready(Err(closed()));
        }
        Pin::new(&mut this.io).poll_read(cx, buf)
    }
}
impl<T: AsyncWrite + Unpin> AsyncWrite for CancellableIo<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        if this.closed(cx) {
            return Poll::Ready(Err(closed()));
        }
        Pin::new(&mut this.io).poll_write(cx, buf)
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if this.closed(cx) {
            return Poll::Ready(Err(closed()));
        }
        Pin::new(&mut this.io).poll_flush(cx)
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().io).poll_shutdown(cx)
    }
}
