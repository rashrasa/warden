use std::{ops::DerefMut, pin::Pin, task::ready};

use tokio::pin;


/// Lazy HTTP connection handle capable of HTTP/1.1 and HTTP/2.
/// 
/// Handles connection state, termination, and re-establishment.
/// Only establishes connection when needed.
#[derive(Debug)]
pub struct Connection {

}

impl Connection {
    pub fn new_http1() -> Self {
        todo!()
    }

    pub fn new_http2() -> Self {
        todo!()
    }

    pub async fn send(&self, req: crate::Request) -> anyhow::Result<crate::IncomingResponse> {
        todo!()
    }
    
    async fn connect(&self) {
        todo!()
    }

    async fn notify_termination(&self) {
        todo!()
    }
}

/// Future holding an in-flight request Future and
/// a copy of its original request for retrying if it fails
/// due to a connection error.
#[derive(Debug)]
pub struct PendingRequest<'a, Fut> {
    // TODO: Consider adding a retry limit.
    connection: &'a Connection,
    req: crate::Request,
    fut: Fut
}

impl<'a,Fut> PendingRequest<'a, Fut> where Fut: Future<Output = anyhow::Result<crate::IncomingResponse>> + Unpin {
    /// Waits until the underlying connection is ready to send requests.
    async fn wait_until_ready(&mut self) {
        todo!()
    }
}

impl<'a, Fut> Future for PendingRequest<'a, Fut> where Fut: Future<Output = anyhow::Result<crate::IncomingResponse>> + Unpin {
    type Output = crate::IncomingResponse;

    /// 1. Poll in-flight request
    /// 2. If resolved, return result
    /// 3. If errored and unretryable return an error response (maybe a 502)
    /// 4. If error is retryable, wait (yield) until retry is possible (new connection gets established in self.connection)
    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        todo!()
    }
}