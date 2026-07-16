use std::{convert::Infallible, net::SocketAddr};

use anyhow::Context;
use http_body_util::Full;
use hyper::{Response, body::Bytes, server::conn::http1, service::service_fn};
use hyper_util::rt::TokioIo;
use log::{error, info, trace};
use tokio::{
    net::{TcpListener, TcpStream},
    select,
};

pub struct Warden {
    host: SocketAddr,
}

impl Warden {
    pub fn new(host: SocketAddr) -> Self {
        Self { host }
    }

    pub async fn serve(&mut self) -> anyhow::Result<()> {
        let listener = TcpListener::bind(self.host).await?;
        info!("server started @ {}", self.host);
        loop {
            select! {
                conn = listener.accept() => {
                    if let Err(e) = self.handle_new_connection(conn).await {
                        error!("{}", e.context("failed to handle new connection"));
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    info!("closing server");
                    break;
                }
            }
        }
        Ok(())
    }

    async fn handle_new_connection(
        &mut self,
        conn: std::io::Result<(TcpStream, SocketAddr)>,
    ) -> anyhow::Result<()> {
        let (stream, addr) = conn.with_context(|| "failed to open connection")?;
        trace!("new connection: {}", addr);
        Connection::spawn(stream).await;

        Ok(())
    }

    async fn hello(
        _: hyper::Request<hyper::body::Incoming>,
    ) -> Result<Response<Full<Bytes>>, Infallible> {
        Ok(Response::new(Full::from(Bytes::from("Hello World"))))
    }
}

struct Connection;

impl Connection {
    async fn spawn(stream: TcpStream) {
        tokio::spawn(async move {
            let io = TokioIo::new(stream);

            if let Err(e) = http1::Builder::new()
                .serve_connection(io, service_fn(Warden::hello))
                .await
            {
                error!(
                    "{}",
                    anyhow::Error::from(e).context("failed to serve request")
                );
            }
        });
    }
}
