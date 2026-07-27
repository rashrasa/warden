mod auth;
pub mod core;
pub mod down;
pub mod up;
pub mod utils;

pub use core::Warden;

use hyper::body::Incoming;

pub const MAX_STATIC_HTML_FILE_SIZE: u64 = 1024 * 1024;

pub type Request = hyper::Request<hyper::body::Incoming>;
pub type IncomingResponse = hyper::Response<Incoming>;
pub type FullResponse = hyper::Response<http_body_util::Full<hyper::body::Bytes>>;
