use std::{convert::Infallible, net::SocketAddr};

use anyhow::Context;
use http_body_util::{BodyExt, Full};
use hyper::{
    Request, Response, Uri,
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

    async fn serve_request(
        request: hyper::Request<hyper::body::Incoming>,
    ) -> Result<Response<Full<Bytes>>, Infallible> {
        let mut path = request.uri().path();
        if let Some(p) = path.strip_suffix("/") {
            path = p;
        }

        match path {
            "" => Warden::hello(request).await,
            "/favicon.ico" => Ok(binary_response(
                include_bytes!("../assets/favicon.ico"),
                "image/x-icon",
            )),
            "/generate_204" => {
                Warden::forward(Uri::from_static("http://google.ca/generate_204"), request).await
            }
            "/placeholder" => {
                Warden::forward(Uri::from_static("http://placehold.co/400"), request).await
            }
            _ => Ok(html_response(include_bytes!("../assets/404.html"))),
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
}

fn binary_response(bytes: &[u8], mime_type: &str) -> Response<Full<Bytes>> {
    Response::builder()
        .header(hyper::header::CONTENT_TYPE, mime_type)
        .body(Full::from(Bytes::from(bytes.to_vec())))
        .unwrap()
}

fn html_response(bytes: &[u8]) -> Response<Full<Bytes>> {
    binary_response(bytes, "text/html")
}
