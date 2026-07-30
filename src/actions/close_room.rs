use crate::pages::room::room_not_found;
use lite_vote::{
    closing::{CloseRoomOutcome, close_room},
    room_creation::CREATOR_COOKIE_NAME,
    security::same_origin,
};
use sqlx::SqlitePool;
use topcoat::{
    Result,
    context::{Cx, app_context, request_context},
    cookie::{Cookies, cookies},
    router::{IntoResponse, Response, StatusCode, path_param, route, see_other},
    view::view,
};

#[path_param]
struct Slug(str);

#[route(POST "/rooms/{slug}/close")]
async fn post_close_room(cx: &Cx) -> Result<Response> {
    if !same_origin(&request_context::<http::request::Parts>(cx).headers) {
        return (StatusCode::FORBIDDEN, "forbidden").into_response(cx);
    }
    let Some(cookie) = cookies(cx).get(CREATOR_COOKIE_NAME) else {
        return (StatusCode::FORBIDDEN, "creator cookie required").into_response(cx);
    };
    let slug = path_param::<Slug>(cx);
    match close_room(app_context::<SqlitePool>(cx), slug, cookie.value()).await {
        Ok(CloseRoomOutcome::Closed) => see_other(&format!("/rooms/{slug}")).into_response(cx),
        Ok(CloseRoomOutcome::NotFound) => {
            let body = view! { room_not_found() }?;
            (StatusCode::NOT_FOUND, body).into_response(cx)
        }
        Ok(CloseRoomOutcome::Forbidden) => {
            (StatusCode::FORBIDDEN, "invalid creator cookie").into_response(cx)
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal server error").into_response(cx),
    }
}
