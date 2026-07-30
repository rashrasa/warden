use anyhow::Context;
use http::Uri;
use hyper::{client::conn::http2::*, service::Service};
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use log::error;
use rustls::{ClientConfig, KeyLogFile, RootCertStore};
use std::{sync::Arc, time::Duration};
use tokio::{net::TcpStream, sync::Mutex};
use tokio_rustls::{TlsConnector, client::TlsStream};

use crate::PinnedFuture;

async fn make_http2_connection(
    io: TokioIo<TlsStream<TcpStream>>,
) -> Result<
    (
        SendRequest<hyper::body::Incoming>,
        Connection<TokioIo<TlsStream<TcpStream>>, hyper::body::Incoming, TokioExecutor>,
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

#[derive(Clone)]
pub struct Http2Upstream {
    inner: Arc<Mutex<Http2UpstreamInner>>,
    connector: TlsConnector,
}

pub struct Http2UpstreamInner {
    sender: SendRequest<hyper::body::Incoming>,
}

impl Http2Upstream {
    pub async fn connect(uri: &Uri) -> anyhow::Result<Self> {
        let root_store = RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.into(),
        };

        let mut config = ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        config.key_log = Arc::new(KeyLogFile::new());

        let connector = TlsConnector::from(Arc::new(config));

        let host = uri.host().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                anyhow::anyhow!("invalid uri: {uri}"),
            )
        })?;
        let address = format!("{host}:443");
        let stream = TcpStream::connect(address.clone()).await?;

        let tls = connector
            .connect(
                address
                    .try_into()
                    .with_context(|| "failed to convert address to ServerName")?,
                stream,
            )
            .await?;

        let io = TokioIo::new(tls);

        let (sender, conn) = make_http2_connection(io).await?;
        tokio::spawn(async move {
            if let Err(e) = conn.await.with_context(|| "connection failed") {
                error!("{e:#}");
            }
        });
        Ok(Self {
            inner: Arc::new(Mutex::new(Http2UpstreamInner { sender })),
            connector,
        })
    }
}

impl std::fmt::Debug for Http2Upstream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{:?}", self.connector.config())
    }
}

impl Service<crate::Request> for Http2Upstream {
    type Response = crate::FullResponse;
    type Error = anyhow::Error;
    type Future = PinnedFuture<Result<Self::Response, Self::Error>>;

    fn call(&self, req: crate::Request) -> Self::Future {
        let cloned = self.clone();
        Box::pin(async move {
            let incoming = cloned
                .inner
                .lock()
                .await
                .sender
                .send_request(req.inner)
                .await?;

            crate::up::collect_body(incoming).await
        })
    }
}
