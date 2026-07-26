use http::StatusCode;
use http_body_util::Full;
use hyper::body::Bytes;
use log::error;

pub fn binary_response(status: StatusCode, body: &[u8], mime_type: &str) -> crate::Response {
    hyper::Response::builder()
        .status(status)
        .header(hyper::header::CONTENT_TYPE, mime_type)
        .body(Full::from(Bytes::from(body.to_vec())))
        .unwrap()
}

pub fn string_response(status: StatusCode, body: &str, mime_type: &str) -> crate::Response {
    hyper::Response::builder()
        .status(status)
        .header(hyper::header::CONTENT_TYPE, mime_type)
        .body(Full::from(Bytes::from(body.as_bytes().to_vec())))
        .unwrap()
}

pub fn html_response(status: StatusCode, html: &str) -> crate::Response {
    string_response(status, html, "text/html")
}

pub fn http_error(code: StatusCode) -> crate::Response {
    html_response(
        code,
        &format!(
            "
        <head></head>
        <body>
            <h1 style=\"text-align: center;\">{code}</h1>
            <hr />
            <p style=\"text-align: center;\">warden/0.1.0</p>
        </body>
            "
        ),
    )
}

pub fn path(request: &crate::Request) -> &str {
    let mut path = request.uri().path();
    if path.len() > 1
        && let Some(p) = path.strip_suffix("/")
    {
        path = p;
    }

    path
}

pub trait WardenHttpExt<T> {
    fn ok_or_500(self) -> Result<T, crate::Response>;
}

impl<T, E> WardenHttpExt<T> for Result<T, E>
where
    E: std::error::Error,
{
    fn ok_or_500(self) -> Result<T, crate::Response> {
        match self {
            Ok(val) => Ok(val),
            Err(err) => {
                error!("{err:?}");

                Err(http_error(StatusCode::INTERNAL_SERVER_ERROR))
            }
        }
    }
}
