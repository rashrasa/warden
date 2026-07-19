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
use std::{convert::Infallible, net::SocketAddr};
use tokio::{
    net::{TcpListener, TcpStream},
    select,
};

use crate::{
    auth::{AuthProvider, DefaultAuthProvider},
    core::{binary_response, path, r_401, r_404, r_500, string_response},
};

pub struct Warden {
    inner: WardenInnerState,
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
    connections: Vec<ConnectionInfo>,
}

impl Warden {
    pub async fn bind(host: SocketAddr) -> anyhow::Result<Self> {
        let listener = TcpListener::bind(host).await?;

        info!("server started @ {}", host);

        Ok(Self {
            inner: WardenInnerState {
                host,
                listener,
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
    ) -> Result<Response<Full<Bytes>>, Infallible> {
        let path = path(&request);

        let verified = DefaultAuthProvider::verify_request(&request);

        match verified {
            Ok(v) => {
                if !v {
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
    ) -> Result<Response<Full<Bytes>>, Infallible> {
        Ok(Response::new(Full::from(Bytes::from(
            "This is an unauthenticated route.\n",
        ))))
    }
    async fn authenticated(
        _: hyper::Request<hyper::body::Incoming>,
    ) -> Result<Response<Full<Bytes>>, Infallible> {
        Ok(Response::new(Full::from(Bytes::from(
            "This is an authenticated route.\n",
        ))))
    }

    async fn forward(
        url: hyper::Uri,
        incoming_request: hyper::Request<Incoming>,
    ) -> Result<Response<Full<Bytes>>, Infallible> {
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
        tokio::spawn(async move {
            let io = TokioIo::new(stream);
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
