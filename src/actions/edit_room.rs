use crate::pages::{
    layout::document,
    room::room_not_found,
    room_edit::{EditRoomForm, edit_form_from_pairs, edit_room_form},
};
use lite_vote::{
    room_creation::CREATOR_COOKIE_NAME,
    room_editing::{EditRoomError, EditRoomInput, EditRoomOutcome, edit_room},
    security::same_origin,
    validation::{ValidationError, ValidationErrorReason, ValidationField},
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

#[route(POST "/rooms/{slug}/edit")]
async fn post_room_edit(cx: &Cx, Form(pairs): Form<Vec<(String, String)>>) -> Result<Response> {
    if !same_origin(&request_context::<http::request::Parts>(cx).headers) {
        return (StatusCode::FORBIDDEN, "forbidden").into_response(cx);
    }
    let Some(cookie) = cookies(cx).get(CREATOR_COOKIE_NAME) else {
        return (StatusCode::FORBIDDEN, "creator cookie required").into_response(cx);
    };
    let slug = path_param::<Slug>(cx);
    let mut form = edit_form_from_pairs(pairs);
    let input = EditRoomInput {
        question: form.question.clone(),
        choices: form.choices.clone(),
    };
    if !(2..=10).contains(&form.submitted_choice_count) {
        form.errors.push("選択肢は2〜10個にしてください。".into());
        let body = view! { document(
            title: "入力エラー - Lite Vote".to_string(),
            edit_room_form(slug: slug.to_owned(), form: form)
        ) }?;
        return (StatusCode::UNPROCESSABLE_ENTITY, body).into_response(cx);
    }
    match edit_room(app_context::<SqlitePool>(cx), slug, cookie.value(), &input).await {
        Ok(EditRoomOutcome::Updated) => see_other(&format!("/rooms/{slug}")).into_response(cx),
        Ok(EditRoomOutcome::NotFound) => {
            let body = view! { room_not_found() }?;
            (StatusCode::NOT_FOUND, body).into_response(cx)
        }
        Ok(EditRoomOutcome::Forbidden) => {
            (StatusCode::FORBIDDEN, "invalid creator cookie").into_response(cx)
        }
        Ok(EditRoomOutcome::VotingStarted | EditRoomOutcome::Closed) => (
            StatusCode::CONFLICT,
            "最初の一票が入ったか、投票が締め切られたため編集できません。",
        )
            .into_response(cx),
        Err(EditRoomError::Validation(errors)) => {
            apply_errors(&mut form, &errors);
            let body = view! { document(
                title: "入力エラー - Lite Vote".to_string(),
                edit_room_form(slug: slug.to_owned(), form: form)
            ) }?;
            (StatusCode::UNPROCESSABLE_ENTITY, body).into_response(cx)
        }
        Err(EditRoomError::Database(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "internal server error").into_response(cx)
        }
    }
}

fn apply_errors(form: &mut EditRoomForm, errors: &[ValidationError]) {
    for error in errors {
        let field_name = match error.field {
            ValidationField::Question => "質問".to_owned(),
            ValidationField::Choice(index) => format!("選択肢 {}", index + 1),
            ValidationField::ChoiceList => "選択肢".to_owned(),
            ValidationField::DisplayName => "入力".to_owned(),
        };
        let suffix = match error.reason {
            ValidationErrorReason::Empty => "を入力してください。".to_owned(),
            ValidationErrorReason::ContainsControlCharacter => {
                "に制御文字は使えません。".to_owned()
            }
            ValidationErrorReason::TooLong { max, .. } => {
                format!("は{max}文字以内で入力してください。")
            }
            ValidationErrorReason::InvalidChoiceCount { .. } => {
                "は2〜10個にしてください。".to_owned()
            }
            ValidationErrorReason::DuplicateChoice { .. } => "が重複しています。".to_owned(),
        };
        let message = format!("{field_name}{suffix}");
        form.errors.push(message.clone());
        match error.field {
            ValidationField::Question => form.question_error = Some(message),
            ValidationField::Choice(index) => {
                if let Some(slot) = form.choice_errors.get_mut(index) {
                    *slot = Some(message);
                }
            }
            ValidationField::ChoiceList | ValidationField::DisplayName => {}
        }
    }
}
