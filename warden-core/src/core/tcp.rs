use std::{
    pin::Pin,
    task::{Poll, ready},
    time::Duration,
};

use tokio::{
    io::{AsyncRead, AsyncWrite},
    time::Instant,
};

/// A tokio IO wrapper with a soft rate limit on reading.
///
/// Lower limits may be exceeded significantly initially but balances out over longer durations.
///
/// Implements [`AsyncWrite`] as a direct passthrough for types that can't easily be separated into read and write parts.
pub struct AsyncRateLimiter<IO> {
    inner: IO,
    limit: i64,
    available: i64,
    last: Instant,
}

impl<IO> AsyncRateLimiter<IO> {
    /// [`bandwidth_limit`] is in bytes per second and must be >0.
    pub fn new(io: IO, bandwidth_limit: i64) -> Self {
        assert!(
            bandwidth_limit > 0,
            "bandwidth_limit must be greater than 0"
        );
        Self {
            inner: io,
            limit: bandwidth_limit,
            available: bandwidth_limit,
            last: Instant::now(),
        }
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
    // Attempts to be smart when adding to the available number of bytes to the pool.
    // The idea is that our allowance shouldn't increase when nothing can be read from the stream.
    // That would result in periods of inactivity being followed by periods of high bandwidth allowance.

    // TODO: Consider cancel safety.
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        // Allow early polling before sleep completes, as it's just a suggestion.
        // As a result we don't need to return early if sleep is pending.

        let now = Instant::now();
        let elapsed_s = (now - self.last).as_secs_f64();
        let available = (self.available + (elapsed_s * self.limit as f64) as i64).min(self.limit);

        if available <= 0 {
            let sleep_dur_secs_min = -(available - 1) as f64 / self.limit as f64;
            // Must sleep until available is back to 1 at least
            let mut sleep = Box::pin(tokio::time::sleep(Duration::from_secs_f64(
                sleep_dur_secs_min,
            )));
            ready!(Pin::poll(Pin::new(&mut sleep), cx));
        }

        let before = buf.filled().len();
        ready!(Pin::new(&mut self.inner).poll_read(cx, buf))?;
        let after = buf.filled().len();

        // likely unnecessary, but provides a helpful guarantee during debugging
        debug_assert!(
            after >= before,
            "poll_read buffer shrunk after calling inner poll_read"
        );

        let read = after - before;
        self.available = available - read as i64;
        self.last = now;

        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use std::task::{Context, Waker};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    #[tokio::test]
    async fn poll_read_throttles() {
        const BANDWIDTH: i64 = 1000;
        const TEST_DUR: usize = 5000;
        const TIME_ADVANCE: u64 = 100;
        const BYTES_SIZE: usize = BANDWIDTH as usize * TEST_DUR;
        static BYTES: [u8; BYTES_SIZE] = [28u8; BYTES_SIZE];
        const STREAM_BUFFER_SIZE: usize = BYTES_SIZE * 2;

        tokio::time::pause();

        let (first, mut second) = tokio::io::duplex(BYTES_SIZE * 2);
        second.write_all(&BYTES).await.unwrap();
        let mut io = AsyncRateLimiter::new(first, BANDWIDTH);

        let mut total_read = 0;
        for _ in 0..TEST_DUR {
            let mut buf = vec![0u8; BANDWIDTH as usize];

            let mut read = Box::pin(io.read(&mut buf));
            let mut cx = Context::from_waker(Waker::noop());
            if let Poll::Ready(r) = Pin::poll(Pin::new(&mut read), &mut cx) {
                let amt = r.unwrap();
                total_read += amt as i64;
            }

            tokio::time::advance(Duration::from_millis(TIME_ADVANCE)).await;
        }
        let expected_value = BANDWIDTH * TEST_DUR as i64;
        let lower_bound = (expected_value - BANDWIDTH).max(0);
        let upper_bound =
            (expected_value + BANDWIDTH).max(expected_value + STREAM_BUFFER_SIZE as i64);

        assert!(
            total_read >= lower_bound && total_read <= upper_bound,
            "failed: {lower_bound} <= {total_read} <= {upper_bound}"
        );
    }
}
