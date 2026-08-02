use std::time::Instant;
use topcoat::{
    Result,
    context::{CxBuilder, request_context},
    router::{Body, Next, Response, layer},
};

#[layer("/")]
async fn request_log(cx: &mut CxBuilder, body: Body, next: Next<'_>) -> Result<Response> {
    let request = request_context::<http::request::Parts>(cx);
    let method = request.method.clone();
    let path = request.uri.path().to_owned();
    let started = Instant::now();

    match next.run(cx, body).await {
        Ok(response) => {
            let status = response.status();
            let elapsed_ms = started.elapsed().as_millis() as u64;
            if status.is_server_error() {
                tracing::error!(%method, %path, status = status.as_u16(), elapsed_ms, "HTTP request failed");
            } else {
                tracing::info!(%method, %path, status = status.as_u16(), elapsed_ms, "HTTP request completed");
            }
            Ok(response)
        }
        Err(error) => {
            let elapsed_ms = started.elapsed().as_millis() as u64;
            tracing::error!(%method, %path, elapsed_ms, error = %error, "HTTP request failed");
            Err(error)
        }
    }
}
