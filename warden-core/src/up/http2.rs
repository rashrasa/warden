use hyper::client::conn::http2::*;
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use std::time::Duration;
use tokio::net::TcpStream;

pub async fn make_http2_connection(
    io: TokioIo<TcpStream>,
) -> Result<
    (
        SendRequest<hyper::body::Incoming>,
        Connection<TokioIo<TcpStream>, hyper::body::Incoming, TokioExecutor>,
    ),
    hyper::Error,
> {
    Builder::new(TokioExecutor::new())
        .keep_alive_while_idle(true)
        .keep_alive_interval(Duration::from_millis(5000))
        .timer(TokioTimer::new())
        .handshake(io)
        .await
}
