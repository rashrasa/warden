pub mod core;
pub mod down;
pub mod services;
pub mod up;
pub mod utils;

pub use core::Warden;
use std::{net::SocketAddr, pin::Pin, sync::PoisonError};

use hyper::body::Incoming;
use log::error;

pub type PinnedFuture<T> = Pin<Box<dyn Future<Output = T> + 'static + Send>>;

pub const MAX_STATIC_HTML_FILE_SIZE: u64 = 1024 * 1024;

pub struct Request {
    pub source: SocketAddr,
    pub inner: RawRequest,
    pub path_extension: String,
}

pub type RawRequest = hyper::Request<hyper::body::Incoming>;

pub type IncomingResponse = hyper::Response<Incoming>;
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
