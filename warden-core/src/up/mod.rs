use std::pin::Pin;

pub mod http1;
pub mod http2;

pub type PinnedFuture<T> = Pin<Box<dyn Future<Output = T> + 'static + Send>>;
