use tauri::http::{
    header::{
        HeaderValue, ALLOW, CACHE_CONTROL, CONTENT_LENGTH, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS,
    },
    Method, Request, Response, StatusCode,
};

pub(crate) const SCHEME: &str = "tauri-plugin-decoration";
pub(crate) const STYLESHEET_PATH: &str = "/controls.css";

const STYLESHEET: &[u8] = include_bytes!("css/controls.css");

pub(crate) fn handle(request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    if !is_stylesheet_route(&request) {
        return plain_response(StatusCode::NOT_FOUND, b"Not Found", None);
    }

    match *request.method() {
        Method::GET => stylesheet_response(STYLESHEET.to_vec()),
        Method::HEAD => stylesheet_response(Vec::new()),
        _ => plain_response(
            StatusCode::METHOD_NOT_ALLOWED,
            b"Method Not Allowed",
            Some("GET, HEAD"),
        ),
    }
}

fn is_stylesheet_route(request: &Request<Vec<u8>>) -> bool {
    let uri = request.uri();
    uri.scheme_str() == Some(SCHEME)
        && uri
            .authority()
            .is_some_and(|authority| authority.as_str() == "localhost")
        && uri
            .path_and_query()
            .is_some_and(|path_and_query| path_and_query.as_str() == STYLESHEET_PATH)
}

fn stylesheet_response(body: Vec<u8>) -> Response<Vec<u8>> {
    let mut response = Response::new(body);
    *response.status_mut() = StatusCode::OK;
    let headers = response.headers_mut();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/css; charset=utf-8"),
    );
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        "Cross-Origin-Resource-Policy",
        HeaderValue::from_static("cross-origin"),
    );
    headers.insert(
        CONTENT_LENGTH,
        HeaderValue::from_str(&STYLESHEET.len().to_string())
            .expect("a decimal usize is always a valid HTTP header value"),
    );
    response
}

fn plain_response(
    status: StatusCode,
    body: &'static [u8],
    allow: Option<&'static str>,
) -> Response<Vec<u8>> {
    let mut response = Response::new(body.to_vec());
    *response.status_mut() = status;
    let headers = response.headers_mut();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Some(allow) = allow {
        headers.insert(ALLOW, HeaderValue::from_static(allow));
    }
    response
}

#[cfg(test)]
mod tests {
    use super::{handle, SCHEME, STYLESHEET, STYLESHEET_PATH};
    use tauri::http::{header, Method, Request, StatusCode};

    fn request(method: Method, uri: &str) -> Request<Vec<u8>> {
        Request::builder()
            .method(method)
            .uri(uri)
            .body(Vec::new())
            .unwrap()
    }

    #[test]
    fn get_serves_only_the_fixed_stylesheet_with_security_headers() {
        let response = handle(request(
            Method::GET,
            &format!("{SCHEME}://localhost{STYLESHEET_PATH}"),
        ));

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.body(), STYLESHEET);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/css; charset=utf-8"
        );
        assert_eq!(
            response
                .headers()
                .get(header::X_CONTENT_TYPE_OPTIONS)
                .unwrap(),
            "nosniff"
        );
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store"
        );
        assert_eq!(
            response
                .headers()
                .get("Cross-Origin-Resource-Policy")
                .unwrap(),
            "cross-origin"
        );
    }

    #[test]
    fn head_has_get_headers_and_no_body() {
        let response = handle(request(
            Method::HEAD,
            &format!("{SCHEME}://localhost{STYLESHEET_PATH}"),
        ));

        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.body().is_empty());
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_LENGTH)
                .unwrap()
                .to_str()
                .unwrap(),
            STYLESHEET.len().to_string()
        );
    }

    #[test]
    fn unsupported_methods_are_rejected_without_reflecting_the_request() {
        let response = handle(request(
            Method::POST,
            &format!("{SCHEME}://localhost{STYLESHEET_PATH}"),
        ));

        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(response.headers().get(header::ALLOW).unwrap(), "GET, HEAD");
        assert_eq!(response.body(), b"Method Not Allowed");
    }

    #[test]
    fn alternate_authority_is_not_routed() {
        for uri in [
            format!("{SCHEME}://evil.example{STYLESHEET_PATH}"),
            format!("{SCHEME}://user@localhost{STYLESHEET_PATH}"),
            format!("{SCHEME}://localhost:443{STYLESHEET_PATH}"),
        ] {
            let response = handle(request(Method::GET, &uri));
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
        }
    }

    #[test]
    fn alternate_scheme_path_or_query_is_not_routed() {
        for uri in [
            format!("other-scheme://localhost{STYLESHEET_PATH}"),
            format!("{SCHEME}://localhost/"),
            format!("{SCHEME}://localhost/controls.css/extra"),
            format!("{SCHEME}://localhost{STYLESHEET_PATH}?v=1"),
        ] {
            let response = handle(request(Method::GET, &uri));
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
            assert_eq!(response.body(), b"Not Found");
        }
    }

    #[test]
    fn stylesheet_exposes_a_computed_style_readiness_sentinel() {
        let css = std::str::from_utf8(STYLESHEET).unwrap();
        assert!(css.contains("--tauri-plugin-decoration-ready: ready"));
    }
}
