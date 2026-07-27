use std::net::SocketAddr;

use anyhow::Context;
use hyper::{server::conn::http2, service::service_fn};
use hyper_util::rt::{TokioExecutor, TokioIo};
use log::{error, trace};
use tokio::net::TcpStream;
use tokio_rustls::TlsAcceptor;

use crate::Warden;

pub struct Downstream {}

impl Downstream {
    pub async fn handle_new_connection(
        warden: Warden,
        acceptor: TlsAcceptor,
        conn: std::io::Result<(TcpStream, SocketAddr)>,
    ) -> anyhow::Result<()> {
        let (stream, addr) = conn.with_context(|| "failed to open connection")?;
        trace!("new connection: {}", addr);

        tokio::spawn(async move {
            let tls_stream = match acceptor
                .accept(stream)
                .await
                .with_context(|| "failed to perform tls handshake")
            {
                Ok(tls_stream) => tls_stream,
                Err(e) => {
                    error!("{e:#}");
                    return;
                }
            };
            let io = TokioIo::new(tls_stream);
            if let Err(e) = http2::Builder::new(TokioExecutor::new())
                .serve_connection(
                    io,
                    service_fn(move |r| {
                        let mut warden = warden.clone();
                        async move { warden.serve_request(r).await }
                    }),
                )
                .await
            {
                error!(
                    "{:#}",
                    anyhow::Error::from(e).context("failed to serve request")
                );
            }
        });

        Ok(())
    }
}
