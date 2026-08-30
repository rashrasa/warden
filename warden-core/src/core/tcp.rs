use std::pin::Pin;

use tokio::io::{AsyncRead, AsyncWrite};

pub struct AsyncRateLimiter<IO> {
    inner: IO,
}

impl<IO> AsyncRateLimiter<IO> {
    pub fn new(io: IO, bandwidth_limit_kbps: u64) -> Self {
        Self { inner: io }
    }
}

impl<IO> AsyncWrite for AsyncRateLimiter<IO>
where
    IO: AsyncWrite + Unpin,
{
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        // TODO
        Pin::new(&mut self.get_mut().inner).poll_write(cx, buf)
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

impl<IO> AsyncRead for AsyncRateLimiter<IO>
where
    IO: AsyncRead + Unpin,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        // TODO
        Pin::new(&mut self.get_mut().inner).poll_read(cx, buf)
    }
}
