use anyhow::Context;
use hyper::{
    server::conn::http2,
    service::{Service, service_fn},
};
use hyper_util::rt::{TokioExecutor, TokioIo};
use log::{debug, error};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

use crate::core::RequestService;

pub struct ConnectionService {
    tcp: TcpListener,
    tls: TlsAcceptor,

    request_service: RequestService,
}

impl ConnectionService {
    pub fn new(tcp: TcpListener, tls: TlsAcceptor, request_service: RequestService) -> Self {
        Self {
            tcp,
            tls,
            request_service,
        }
    }

    pub async fn serve_next_connection(&mut self) -> anyhow::Result<()> {
        let (stream, addr) = self
            .tcp
            .accept()
            .await
            .with_context(|| "failed to create tcp connection")?;

        debug!("new connection: {}", addr);
        let tls = self.tls.clone();
        let request_service = self.request_service.clone();
        tokio::spawn(async move {
            let tls_stream = match tls
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

            let service = service_fn(|req: crate::RawRequest| {
                request_service.call(crate::Request {
                    source: addr,
                    inner: req,
                    path_extension: String::new(),
                })
            });

            if let Err(e) = http2::Builder::new(TokioExecutor::new())
                .serve_connection(io, service)
                .await
            {
                error!("{:#}", anyhow::Error::from(e).context("connection failed"));
            }
        });
        Ok(())
    }
}
