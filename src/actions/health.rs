use lite_vote::db::validate_migrations;
use sqlx::SqlitePool;
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{IntoResponse, Response, StatusCode, route},
};

#[route(GET "/healthz")]
async fn healthz(cx: &Cx) -> Result<Response> {
    (StatusCode::OK, "ok").into_response(cx)
}

#[route(GET "/readyz")]
async fn readyz(cx: &Cx) -> Result<Response> {
    let pool = app_context::<SqlitePool>(cx);
    let database_healthy = sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(pool)
        .await
        .is_ok();
    let migrations_current = database_healthy && validate_migrations(pool).await.is_ok();

    if migrations_current {
        (StatusCode::OK, "ready").into_response(cx)
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "not ready").into_response(cx)
    }
}
