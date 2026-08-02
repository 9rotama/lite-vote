use crate::pages::room::{results_region, room_not_found};
use lite_vote::{
    participant_entry::PARTICIPANT_COOKIE_NAME,
    participant_entry::load_room,
    realtime::RoomUpdateHub,
    results::load_results,
    security::same_origin,
    voting::{VoteOutcome, cast_vote},
};
use sqlx::SqlitePool;
use topcoat::{
    Result,
    context::{Cx, app_context, request_context},
    cookie::{Cookies, cookies},
    router::{Form, IntoResponse, Response, StatusCode, path_param, route, see_other},
    view::view,
};

#[path_param]
struct Slug(str);

#[route(POST "/rooms/{slug}/votes")]
async fn post_vote(cx: &Cx, Form(pairs): Form<Vec<(String, String)>>) -> Result<Response> {
    if !same_origin(&request_context::<http::request::Parts>(cx).headers) {
        return (StatusCode::FORBIDDEN, "forbidden").into_response(cx);
    }
    let Some(participant_cookie) = cookies(cx).get(PARTICIPANT_COOKIE_NAME) else {
        return (StatusCode::FORBIDDEN, "participant cookie required").into_response(cx);
    };
    let Some(choice_id) = pairs
        .into_iter()
        .find_map(|(name, value)| (name == "choice_id").then_some(value))
        .and_then(|value| value.parse::<i64>().ok())
    else {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            "投票先を選択してください。",
        )
            .into_response(cx);
    };

    let slug = path_param::<Slug>(cx);
    let pool = app_context::<SqlitePool>(cx);
    match cast_vote(pool, slug, participant_cookie.value(), choice_id).await {
        Ok(VoteOutcome::Recorded) => {
            tracing::info!(room_slug = %slug, "vote recorded");
            app_context::<RoomUpdateHub>(cx).notify(slug);
            let wants_results = request_context::<http::request::Parts>(cx)
                .headers
                .get("x-lite-vote-partial")
                .is_some_and(|value| value == "results");
            if !wants_results {
                return see_other(&format!("/rooms/{slug}")).into_response(cx);
            }
            let Some(details) = load_room(pool, slug).await.map_err(topcoat::Error::from)? else {
                let body = view! { room_not_found() }?;
                return (StatusCode::NOT_FOUND, body).into_response(cx);
            };
            let results = load_results(
                pool,
                details.room.id,
                details.room.is_closed(),
                details.room.participant_names_public,
            )
            .await
            .map_err(topcoat::Error::from)?;
            let body = view! {
                results_region(
                    results: results,
                    closed: details.room.is_closed(),
                    participant_names_public: details.room.participant_names_public,
                )
            }?;
            body.into_response(cx)
        }
        Ok(VoteOutcome::Closed) => {
            (StatusCode::CONFLICT, "この投票は締め切られています。").into_response(cx)
        }
        Ok(VoteOutcome::RoomNotFound) => {
            let body = view! { room_not_found() }?;
            (StatusCode::NOT_FOUND, body).into_response(cx)
        }
        Ok(VoteOutcome::ParticipantNotFound) => {
            (StatusCode::FORBIDDEN, "invalid participant cookie").into_response(cx)
        }
        Ok(VoteOutcome::ChoiceNotFound) => {
            (StatusCode::UNPROCESSABLE_ENTITY, "投票先が見つかりません。").into_response(cx)
        }
        Err(error) => {
            tracing::error!(room_slug = %slug, error = %error, "vote failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "internal server error").into_response(cx)
        }
    }
}
