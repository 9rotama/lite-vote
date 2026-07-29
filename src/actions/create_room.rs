use crate::pages::{
    home::{RoomForm, create_form, form_from_pairs},
    layout::document,
};
use lite_vote::{
    room_creation::{
        CREATOR_COOKIE_MAX_AGE_SECONDS, CREATOR_COOKIE_NAME, CreateRoomError, CreateRoomInput,
        CreateRoomValidationError, create_room, validate_create_room,
    },
    validation::{ValidationErrorReason, ValidationField},
};
use sqlx::SqlitePool;
use topcoat::{
    Result,
    context::{Cx, app_context, request_context},
    cookie::{Cookie, Cookies, SameSite, cookies, time::Duration},
    router::{Form, IntoResponse, Response, StatusCode, route, see_other},
    view::view,
};

#[route(POST "/rooms")]
async fn post_rooms(cx: &Cx, Form(pairs): Form<Vec<(String, String)>>) -> Result<Response> {
    if !same_origin(cx) {
        return (StatusCode::FORBIDDEN, "forbidden").into_response(cx);
    }
    let mut form = form_from_pairs(pairs);
    let input = CreateRoomInput {
        question: form.question.clone(),
        choices: form.choices.clone(),
        visibility: form.visibility.clone(),
    };
    if !(2..=10).contains(&form.submitted_choice_count) {
        form.errors
            .push("選択肢は2〜10個にしてください。".to_string());
        if let Err(errors) = validate_create_room(&input) {
            apply_field_errors(&mut form, &errors);
            form.errors.extend(error_messages(&errors));
        }
        let body = view! { document(
            title: "入力エラー - Lite Vote".to_string(),
            create_form(form: form)
        ) }?;
        return (StatusCode::UNPROCESSABLE_ENTITY, body).into_response(cx);
    }
    match create_room(app_context::<SqlitePool>(cx), &input).await {
        Ok(created) => {
            let path = format!("/rooms/{}", created.slug);
            cookies(cx).add(
                Cookie::build((CREATOR_COOKIE_NAME, created.creator_token))
                    .path(path.clone())
                    .http_only(true)
                    .secure(true)
                    .same_site(SameSite::Lax)
                    .max_age(Duration::seconds(CREATOR_COOKIE_MAX_AGE_SECONDS))
                    .build(),
            );
            see_other(&path).into_response(cx)
        }
        Err(CreateRoomError::Validation(errors)) => {
            apply_field_errors(&mut form, &errors);
            form.errors = error_messages(&errors);
            let body = view! { document(
                title: "入力エラー - Lite Vote".to_string(),
                create_form(form: form)
            ) }?;
            (StatusCode::UNPROCESSABLE_ENTITY, body).into_response(cx)
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal server error").into_response(cx),
    }
}

fn same_origin(cx: &Cx) -> bool {
    let parts = request_context::<http::request::Parts>(cx);
    let Some(origin) = parts
        .headers
        .get("origin")
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let Some(host) = parts
        .headers
        .get("host")
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    origin == format!("https://{host}") || origin == format!("http://{host}")
}

fn error_messages(errors: &[CreateRoomValidationError]) -> Vec<String> {
    let mut messages = Vec::new();
    for error in errors {
        match error {
            CreateRoomValidationError::Visibility => {
                messages.push("投票者名を公開するか選択してください。".into());
            }
            CreateRoomValidationError::Fields(fields) => {
                for field in fields {
                    let name = match field.field {
                        ValidationField::Question => "質問".to_string(),
                        ValidationField::Choice(index) => format!("選択肢 {}", index + 1),
                        ValidationField::ChoiceList => "選択肢".to_string(),
                        ValidationField::DisplayName => "表示名".to_string(),
                    };
                    let reason = match field.reason {
                        ValidationErrorReason::Empty => "を入力してください。".to_string(),
                        ValidationErrorReason::ContainsControlCharacter => {
                            "に制御文字は使えません。".to_string()
                        }
                        ValidationErrorReason::TooLong { max, .. } => {
                            format!("は{max}文字以内で入力してください。")
                        }
                        ValidationErrorReason::InvalidChoiceCount { .. } => {
                            "は2〜10個にしてください。".to_string()
                        }
                        ValidationErrorReason::DuplicateChoice { .. } => {
                            "が重複しています。".to_string()
                        }
                    };
                    messages.push(format!("{name}{reason}"));
                }
            }
        }
    }
    messages
}

fn apply_field_errors(form: &mut RoomForm, errors: &[CreateRoomValidationError]) {
    for error in errors {
        match error {
            CreateRoomValidationError::Visibility => {
                form.visibility_error = Some("投票者名を公開するか選択してください。".to_string());
            }
            CreateRoomValidationError::Fields(fields) => {
                for field in fields {
                    let message =
                        error_messages(&[CreateRoomValidationError::Fields(vec![field.clone()])])
                            .into_iter()
                            .next()
                            .expect("one field error produces one message");
                    match field.field {
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
        }
    }
}

#[cfg(test)]
mod tests;
