use http_body_util::Full;
use hyper::{Request, Response, StatusCode, body::Bytes};

pub fn binary_response(status: StatusCode, body: &[u8], mime_type: &str) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header(hyper::header::CONTENT_TYPE, mime_type)
        .body(Full::from(Bytes::from(body.to_vec())))
        .unwrap()
}

pub fn string_response(status: StatusCode, body: &str, mime_type: &str) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header(hyper::header::CONTENT_TYPE, mime_type)
        .body(Full::from(Bytes::from(body.as_bytes().to_vec())))
        .unwrap()
}

pub fn html_response(status: StatusCode, html: &str) -> Response<Full<Bytes>> {
    string_response(status, html, "text/html")
}

pub fn r_401() -> Response<Full<Bytes>> {
    html_response(
        StatusCode::UNAUTHORIZED,
        &(include_str!("../assets/401.html").to_string() + "\n"),
    )
}

pub fn r_404() -> Response<Full<Bytes>> {
    html_response(
        StatusCode::NOT_FOUND,
        &(include_str!("../assets/404.html").to_string() + "\n"),
    )
}

pub fn r_500() -> Response<Full<Bytes>> {
    html_response(
        StatusCode::NOT_FOUND,
        &(include_str!("../assets/500.html").to_string() + "\n"),
    )
}

pub fn path<T>(request: &Request<T>) -> &str {
    let mut path = request.uri().path();
    if let Some(p) = path.strip_suffix("/") {
        path = p;
    }

    path
}
