use http::StatusCode;
use http_body_util::Full;
use hyper::body::Bytes;

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

pub fn r_401() -> crate::Response {
    html_response(
        StatusCode::UNAUTHORIZED,
        &(include_str!("../assets/401.html").to_string() + "\n"),
    )
}

pub fn r_404() -> crate::Response {
    html_response(
        StatusCode::NOT_FOUND,
        &(include_str!("../assets/404.html").to_string() + "\n"),
    )
}

pub fn r_500() -> crate::Response {
    html_response(
        StatusCode::NOT_FOUND,
        &(include_str!("../assets/500.html").to_string() + "\n"),
    )
}

pub fn path(request: &crate::Request) -> &str {
    let mut path = request.uri().path();
    if let Some(p) = path.strip_suffix("/") {
        path = p;
    }

    path
}
