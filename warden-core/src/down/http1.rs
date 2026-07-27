use hyper::client::conn::http1::*;
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;

pub async fn make_http1_connection(
    io: TokioIo<TcpStream>,
) -> Result<
    (
        SendRequest<hyper::body::Incoming>,
        Connection<TokioIo<TcpStream>, hyper::body::Incoming>,
    ),
    hyper::Error,
> {
    Builder::new().handshake(io).await
}
