use super::*;
use topcoat::context::CxTestBuilder;

fn request_context(origin: Option<&str>, host: Option<&str>) -> Cx {
    let mut request = http::Request::new(());
    if let Some(origin) = origin {
        request
            .headers_mut()
            .insert("origin", origin.parse().unwrap());
    }
    if let Some(host) = host {
        request.headers_mut().insert("host", host.parse().unwrap());
    }
    CxTestBuilder::new()
        .request_context(request.into_parts().0)
        .build()
}

#[test]
fn origin_must_exactly_match_the_http_or_https_host() {
    assert!(same_origin(&request_context(
        Some("https://vote.example"),
        Some("vote.example"),
    )));
    assert!(same_origin(&request_context(
        Some("http://127.0.0.1:3000"),
        Some("127.0.0.1:3000"),
    )));
    assert!(!same_origin(&request_context(
        Some("https://evil.example"),
        Some("vote.example"),
    )));
    assert!(!same_origin(&request_context(
        Some("https://vote.example.evil.example"),
        Some("vote.example"),
    )));
    assert!(!same_origin(&request_context(None, Some("vote.example"))));
    assert!(!same_origin(&request_context(
        Some("https://vote.example"),
        None,
    )));
}
