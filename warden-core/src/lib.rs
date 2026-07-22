mod auth;
mod core;
mod router;

use anyhow::Context;
use http_body_util::{BodyExt, Full};
use hyper::{
    Request, Response, StatusCode, Uri,
    body::{Bytes, Incoming},
    server::conn::http1,
    service::service_fn,
};
use hyper_util::rt::TokioIo;
use log::{error, info, trace};
use rustls::{
    ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
};
use std::{net::SocketAddr, pin::Pin, sync::Arc, task::Poll};
use tokio::{
    net::{TcpListener, TcpStream},
    select,
};
use tokio_rustls::TlsAcceptor;
use tower::Service;

use crate::{
    auth::{AuthProvider, Authorization, DefaultAuthProvider},
    core::{binary_response, path, r_401, r_404, r_500, string_response},
};

pub struct Warden {
    inner: WardenInnerState,
}

impl Service<Request<Incoming>> for Warden {
    type Response = Response<Full<Bytes>>;
    type Error = anyhow::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        // TODO: Implement rate limiting here or somewhere else
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Incoming>) -> Self::Future {
        Box::pin(Self::serve_request(req))
    }
}

#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    pub host: SocketAddr,
    pub user_agent: Option<String>,
}

// Tasks:
//   - Accept connections and spawn handler
//   - Perform health checks
//   - Wait for termination signal
struct WardenInnerState {
    host: SocketAddr,
    listener: TcpListener,
    tls_acceptor: TlsAcceptor,
    connections: Vec<ConnectionInfo>,
}

impl Warden {
    pub async fn bind(host: SocketAddr) -> anyhow::Result<Self> {
        // Setup TLS
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

        let certs = CertificateDer::pem_file_iter("temp/server.crt")?
            .collect::<Result<Vec<_>, _>>()
            .with_context(|| "failed to read cert file")?;

        let key = PrivateKeyDer::from_pem_file("temp/server.pem")
            .with_context(|| "failed to read private key file")?;

        let mut server_config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .with_context(|| "failed to create TLS server config")?;

        server_config.alpn_protocols =
            vec![b"h2".to_vec(), b"http/1.1".to_vec(), b"http/1.0".to_vec()];
        let tls_acceptor = TlsAcceptor::from(Arc::new(server_config));

        let listener = TcpListener::bind(host).await?;

        info!("server started @ {}", host);

        Ok(Self {
            inner: WardenInnerState {
                host,
                listener,
                tls_acceptor,
                connections: vec![],
            },
        })
    }

    pub async fn serve_next(&mut self) -> anyhow::Result<()> {
        select! {
            conn = self.inner.listener.accept() => {
                if let Err(e) = self.handle_new_connection(conn).await {
                    error!("{}", e.context("failed to handle new connection"));
                }
                Ok(())
            }
        }
    }

    pub fn host(&self) -> &SocketAddr {
        &self.inner.host
    }

    /// This drives the gateway until receiving a termination signal in the shell
    /// that started it.
    pub async fn serve_async(&mut self) -> anyhow::Result<()> {
        loop {
            self.serve_next().await?;
        }
    }

    async fn serve_request(
        request: hyper::Request<hyper::body::Incoming>,
    ) -> anyhow::Result<Response<Full<Bytes>>> {
        let path = path(&request);

        let verified = DefaultAuthProvider::verify_request(&request);

        match verified {
            Ok(v) => {
                if let Authorization::Blocked = v {
                    return Ok(r_401());
                }
            }
            Err(e) => {
                error!("{}", e.context("error verifying request"));
                return Ok(r_500());
            }
        }

        match path {
            "" => Warden::hello(request).await,
            "/authenticated" => Warden::authenticated(request).await,
            "/favicon.ico" => Ok(binary_response(
                StatusCode::OK,
                include_bytes!("../assets/favicon.ico"),
                "image/x-icon",
            )),
            "/status" => Ok(string_response(StatusCode::OK, "Healthy", "text/plain")),
            "/generate_204" => {
                Warden::forward(Uri::from_static("http://google.ca/generate_204"), request).await
            }
            "/placeholder" => {
                Warden::forward(Uri::from_static("http://placehold.co/400"), request).await
            }
            _ => Ok(r_404()),
        }
    }

    async fn hello(
        _: hyper::Request<hyper::body::Incoming>,
    ) -> anyhow::Result<Response<Full<Bytes>>> {
        Ok(Response::new(Full::from(Bytes::from(
            "This is an unauthenticated route.\n",
        ))))
    }
    async fn authenticated(
        _: hyper::Request<hyper::body::Incoming>,
    ) -> anyhow::Result<Response<Full<Bytes>>> {
        Ok(Response::new(Full::from(Bytes::from(
            "This is an authenticated route.\n",
        ))))
    }

    async fn forward(
        url: hyper::Uri,
        incoming_request: hyper::Request<Incoming>,
    ) -> anyhow::Result<Response<Full<Bytes>>> {
        let ip = url.host().unwrap();
        let port = url.port_u16().unwrap_or(80);
        let authority = url.authority().unwrap().clone();

        // TODO: Handle errors cleanly, ensuring internal errors like host parsing and connection establishment are returned as generic 500s

        let stream = TcpStream::connect(format!("{}:{}", ip, port))
            .await
            .unwrap();

        let io = TokioIo::new(stream);
        let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await.unwrap();

        tokio::spawn(async move {
            if let Err(e) = conn.await {
                error!("connection failed: {:?}", e)
            }
        });
        let mut request = Request::builder()
            .uri(&url)
            .method(incoming_request.method())
            .header(hyper::header::HOST, authority.as_str());

        for (name, value) in incoming_request.headers() {
            if name != hyper::header::HOST {
                request = request.header(name, value);
            }
        }

        let request = request.body(incoming_request.into_body()).unwrap();
        let headers = request.headers().clone();

        let (parts, body) = sender
            .send_request(request)
            .await
            .with_context(|| format!("failed to send request with headers: {:?}", headers))
            .unwrap()
            .into_parts();
        let body = body.collect().await.unwrap().to_bytes();

        trace!(
            "received response from {url}:\n\nStatus: {}\nBody: {}",
            parts.status.clone(),
            String::from_utf8(body.to_vec()).unwrap()
        );

        let response = Response::builder()
            .status(parts.status)
            .body(Full::from(body))
            .unwrap();
        Ok(response)
    }

    async fn handle_new_connection(
        &mut self,
        conn: std::io::Result<(TcpStream, SocketAddr)>,
    ) -> anyhow::Result<()> {
        let (stream, addr) = conn.with_context(|| "failed to open connection")?;
        trace!("new connection: {}", addr);
        self.inner.connections.push(ConnectionInfo {
            host: addr,
            user_agent: None,
        });
        let acceptor = self.inner.tls_acceptor.clone();
        tokio::spawn(async move {
            let tls_stream = match acceptor
                .accept(stream)
                .await
                .with_context(|| "failed to perform tls handshake")
            {
                Ok(tls_stream) => tls_stream,
                Err(e) => {
                    error!("{e}");
                    return;
                }
            };
            let io = TokioIo::new(tls_stream);
            if let Err(e) = http1::Builder::new()
                .serve_connection(io, service_fn(Warden::serve_request))
                .await
            {
                error!(
                    "{}",
                    anyhow::Error::from(e).context("failed to serve request")
                );
            }
        });

        Ok(())
    }

    pub fn connections(&self) -> &[ConnectionInfo] {
        &self.inner.connections
    }
}
