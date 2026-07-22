mod auth;
pub mod core;
mod router;
pub mod utils;

pub use core::Warden;

pub type Request = hyper::Request<hyper::body::Incoming>;
pub type Response = hyper::Response<http_body_util::Full<hyper::body::Bytes>>;
pub type Error = anyhow::Error;
pub type StaticFuture =
    std::pin::Pin<Box<dyn Future<Output = Result<Request, Response>> + 'static + Send>>;
