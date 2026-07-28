use http::{HeaderMap, StatusCode};
use http_body_util::Full;
use hyper::body::Bytes;
use log::error;

pub fn binary_response(status: StatusCode, body: &[u8], mime_type: &str) -> crate::FullResponse {
    hyper::Response::builder()
        .status(status)
        .header(hyper::header::CONTENT_TYPE, mime_type)
        .body(Full::from(Bytes::from(body.to_vec())))
        .unwrap()
}

pub fn string_response(status: StatusCode, body: &str, mime_type: &str) -> crate::FullResponse {
    hyper::Response::builder()
        .status(status)
        .header(hyper::header::CONTENT_TYPE, mime_type)
        .body(Full::from(Bytes::from(body.as_bytes().to_vec())))
        .unwrap()
}

pub fn html_response(status: StatusCode, html: &str) -> crate::FullResponse {
    string_response(status, html, "text/html")
}

pub fn http_error_with_headers(
    code: StatusCode,
    additional_headers: HeaderMap,
) -> crate::FullResponse {
    let mut response = html_response(
        code,
        &format!(
            "
        <head></head>
        <body style=\"text-align: center;font-family:\'Trebuchet MS\',Helvetica,\'Times New Roman\',monospace;\">
            <h1>{code}</h1>
            <hr />
            <p>warden/{}</p>
        </body>
            ",
            env!("CARGO_PKG_VERSION")
        ),
    );

    let headers = response.headers_mut();

    for (name, value) in additional_headers.into_iter() {
        if let Some(name) = name {
            headers.insert(name, value);
        }
    }

    response
}

pub fn http_error(code: StatusCode) -> crate::FullResponse {
    http_error_with_headers(code, HeaderMap::new())
}

pub fn path(request: &crate::Request) -> &str {
    let mut path = request.inner.uri().path();
    if path.len() > 1
        && let Some(p) = path.strip_suffix("/")
    {
        path = p;
    }

    path
}

pub trait WardenHttpExt<T> {
    fn ok_or_500(self) -> Result<T, crate::FullResponse>;
}

impl<T, E> WardenHttpExt<T> for Result<T, E>
where
    E: std::error::Error,
{
    fn ok_or_500(self) -> Result<T, crate::FullResponse> {
        match self {
            Ok(val) => Ok(val),
            Err(err) => {
                error!("{err:?}");

                Err(http_error(StatusCode::INTERNAL_SERVER_ERROR))
            }
        }
    }
}
