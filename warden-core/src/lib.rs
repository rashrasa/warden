mod auth;
pub mod core;
pub mod down;
pub mod throttle;
pub mod up;
pub mod utils;

pub use core::Warden;
use std::net::SocketAddr;

use hyper::body::Incoming;

pub const MAX_STATIC_HTML_FILE_SIZE: u64 = 1024 * 1024;

pub struct Request {
    pub source: SocketAddr,
    pub inner: RawRequest,
}

pub type RawRequest = hyper::Request<hyper::body::Incoming>;

pub type IncomingResponse = hyper::Response<Incoming>;
pub type FullResponse = hyper::Response<http_body_util::Full<hyper::body::Bytes>>;
