use crate::pages::{
    layout::document,
    room::{
        ParticipantForm, display_name_error, participant_form, room_not_found,
        set_participant_cookie,
    },
};
use lite_vote::{
    participant_entry::{
        EntryError, EntryKind, EntryOutcome, PARTICIPANT_COOKIE_NAME, create_participant,
        find_participant_by_token, load_room,
    },
    security::same_origin,
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

#[route(POST "/rooms/{slug}/participants")]
async fn post_participant(cx: &Cx, Form(pairs): Form<Vec<(String, String)>>) -> Result<Response> {
    if !same_origin(&request_context::<http::request::Parts>(cx).headers) {
        return (StatusCode::FORBIDDEN, "forbidden").into_response(cx);
    }
    let slug = path_param::<Slug>(cx);
    let pool = app_context::<SqlitePool>(cx);
    let Some(details) = load_room(pool, slug).await.map_err(topcoat::Error::from)? else {
        let body = view! { room_not_found() }?;
        return (StatusCode::NOT_FOUND, body).into_response(cx);
    };
    let path = format!("/rooms/{slug}");

    if let Some(cookie) = cookies(cx).get(PARTICIPANT_COOKIE_NAME)
        && find_participant_by_token(pool, details.room.id, cookie.value())
            .await
            .map_err(topcoat::Error::from)?
            .is_some()
    {
        set_participant_cookie(cx, slug, cookie.value());
        return see_other(&path).into_response(cx);
    }
    if details.room.is_closed() || !details.room.participant_names_public {
        return see_other(&path).into_response(cx);
    }

    let display_name = pairs
        .into_iter()
        .find_map(|(name, value)| (name == "display_name").then_some(value))
        .unwrap_or_default();
    match create_participant(pool, slug, EntryKind::Public(&display_name)).await {
        Ok(EntryOutcome::Created { token, .. }) => {
            set_participant_cookie(cx, slug, &token);
            see_other(&path).into_response(cx)
        }
        Ok(EntryOutcome::Closed | EntryOutcome::VisibilityChanged) => {
            see_other(&path).into_response(cx)
        }
        Ok(EntryOutcome::NotFound) => {
            let body = view! { room_not_found() }?;
            (StatusCode::NOT_FOUND, body).into_response(cx)
        }
        Err(EntryError::Validation(error)) => {
            let title = format!("入力エラー - {}", details.room.question);
            let body = view! { document(title: title, participant_form(
                details: details,
                form: ParticipantForm {
                    display_name,
                    error: Some(display_name_error(&error)),
                },
                creator_can_edit: false,
            )) }?;
            (StatusCode::UNPROCESSABLE_ENTITY, body).into_response(cx)
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal server error").into_response(cx),
    }
}
