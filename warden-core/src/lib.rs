pub mod core;
pub mod down;
pub mod services;
pub mod up;
pub mod utils;

pub use core::Warden;
use std::{pin::Pin, sync::PoisonError};

use log::error;

pub type PinnedFuture<T> = Pin<Box<dyn Future<Output = T> + 'static + Send>>;

pub const DEFAULT_HEADER_SIZE_MAX: u32 = 8 * 1024;

/// At least 100 as recommended in the [HTTP/2 RFC](https://httpwg.org/specs/rfc9113.html#SETTINGS_MAX_CONCURRENT_STREAMS)
pub const DEFAULT_CONNECTION_CONCURRENT_REQUESTS_MAX: u32 = 200;

pub struct Request {
    pub inner: RawRequest,
    pub path_extension: String,
}

pub type RawRequest = hyper::Request<hyper::body::Incoming>;

pub type IncomingResponse = hyper::Response<hyper::body::Incoming>;
pub type FullResponse = hyper::Response<http_body_util::Full<hyper::body::Bytes>>;

pub trait UnwrapLog<T> {
    fn unwrap_log(self) -> T;
}

impl<T> UnwrapLog<T> for Result<T, PoisonError<T>> {
    fn unwrap_log(self) -> T {
        match self {
            Ok(v) => v,
            Err(err) => {
                error!("{err:?}",);
                err.into_inner()
            }
        }
    }
}
