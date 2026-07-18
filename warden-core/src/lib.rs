use std::{convert::Infallible, net::SocketAddr};

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
use tokio::{
    net::{TcpListener, TcpStream},
    select,
};

const USER_HEADER: &str = "x-warden-user";
const AUTHORIZED_USERS: [&str; 2] = ["user1", "user2"];

pub struct Warden {
    inner: WardenInnerState,
}

// Tasks:
//   - Accept connections and spawn handler
//   - Perform health checks
//   - Wait for termination signal
struct WardenInnerState {
    host: SocketAddr,
    listener: TcpListener,
    lifetime_connections: usize,
}

impl Warden {
    pub async fn bind(host: SocketAddr) -> anyhow::Result<Self> {
        let listener = TcpListener::bind(host).await?;

        info!("server started @ {}", host);
        Ok(Self {
            inner: WardenInnerState {
                host,
                listener,
                lifetime_connections: 0,
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

    pub fn is_healthy(&self) -> bool {
        // TODO: implement health check for gateway
        true
    }

    /// This drives the gateway until receiving a termination signal in the shell
    /// that started it.
    pub async fn serve_forever(&mut self) -> anyhow::Result<()> {
        while let Ok(_) = self.serve_next().await {}

        Ok(())
    }

    fn verify_request(
        request: &hyper::Request<hyper::body::Incoming>,
    ) -> Result<(), Response<Full<Bytes>>> {
        let path = path(request);
        match path {
            "/favicon.ico" => return Ok(()),
            _ => {}
        }
        match request.headers().get(USER_HEADER) {
            None => return Err(r_401()),
            Some(user) => {
                if let Ok(user_str) = String::from_utf8(user.as_bytes().to_vec()) {
                    if !AUTHORIZED_USERS.contains(&user_str.as_str()) {
                        return Err(r_401());
                    }
                } else {
                    return Err(r_401());
                }
            }
        }
        return Ok(());
    }

    async fn serve_request(
        request: hyper::Request<hyper::body::Incoming>,
    ) -> Result<Response<Full<Bytes>>, Infallible> {
        if let Err(response) = Self::verify_request(&request) {
            return Ok(response);
        }

        let path = path(&request);
        match path {
            "" => Warden::hello(request).await,
            "/favicon.ico" => Ok(binary_response(
                StatusCode::OK,
                include_bytes!("../assets/favicon.ico"),
                "image/x-icon",
            )),
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
        Ok(Response::new(Full::from(Bytes::from("Hello World"))))
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
        self.spawn_connection_handler(stream).await;
        self.inner.lifetime_connections += 1;

        Ok(())
    }

    async fn spawn_connection_handler(&mut self, stream: TcpStream) {
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
    }

    pub fn lifetime_connections(&self) -> usize {
        self.inner.lifetime_connections
    }
}

fn binary_response(status: StatusCode, bytes: &[u8], mime_type: &str) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header(hyper::header::CONTENT_TYPE, mime_type)
        .body(Full::from(Bytes::from(bytes.to_vec())))
        .unwrap()
}

fn html_response(status: StatusCode, bytes: &[u8]) -> Response<Full<Bytes>> {
    binary_response(status, bytes, "text/html")
}

fn r_401() -> Response<Full<Bytes>> {
    html_response(
        StatusCode::UNAUTHORIZED,
        include_bytes!("../assets/401.html"),
    )
}

fn r_404() -> Response<Full<Bytes>> {
    html_response(StatusCode::NOT_FOUND, include_bytes!("../assets/404.html"))
}

fn path<T>(request: &Request<T>) -> &str {
    let mut path = request.uri().path();
    if let Some(p) = path.strip_suffix("/") {
        path = p;
    }

    path
}
