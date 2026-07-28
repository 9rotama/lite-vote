mod components;

use anyhow::{Context, Result as AnyResult};
use components::button::{ButtonVariant, button};
use components::{input::input as input_component, label::label, textarea::textarea};
use lite_vote::{
    db::{DatabaseConfig, connect},
    room_creation::{
        CREATOR_COOKIE_MAX_AGE_SECONDS, CREATOR_COOKIE_NAME, CreateRoomError, CreateRoomInput,
        create_room, hash_token, validate_create_room,
    },
    validation::{ValidationErrorReason, ValidationField},
};
use sqlx::SqlitePool;
use topcoat::{
    Result,
    asset::{AssetBundle, RouterBuilderAssetExt},
    context::{Cx, app_context, request_context},
    cookie::{Cookie, Cookies, RouterBuilderCookieExt, SameSite, cookies, time::Duration},
    font::fontsource::fontsource_font,
    router::{
        Form, IntoResponse, Response, Router, RouterBuilderDiscoverExt, StatusCode, page,
        path_param, route, see_other,
    },
    runtime::Event,
    tailwind,
    view::{attributes, component, view},
};

#[tokio::main]
async fn main() -> AnyResult<()> {
    let database = connect(DatabaseConfig::from_env()?)
        .await
        .context("database startup check failed")?;
    let router = Router::builder()
        .discover()
        .assets(AssetBundle::load().unwrap())
        .app_context(database.pool)
        .cookies()
        .build();
    topcoat::start(router).await?;
    Ok(())
}

#[derive(Clone, Debug)]
struct RoomForm {
    question: String,
    choices: Vec<String>,
    visibility: Option<String>,
    errors: Vec<String>,
    submitted_choice_count: usize,
    question_error: Option<String>,
    choice_errors: Vec<Option<String>>,
    visibility_error: Option<String>,
}

impl Default for RoomForm {
    fn default() -> Self {
        Self {
            question: String::new(),
            choices: vec![String::new(), String::new()],
            visibility: None,
            errors: Vec::new(),
            submitted_choice_count: 2,
            question_error: None,
            choice_errors: vec![None; 10],
            visibility_error: None,
        }
    }
}

fn form_from_pairs(pairs: Vec<(String, String)>) -> RoomForm {
    let mut form = RoomForm::default();
    form.choices.clear();
    for (name, value) in pairs {
        match name.as_str() {
            "question" => form.question = value,
            "choice" => form.choices.push(value),
            "visibility" => form.visibility = Some(value),
            _ => {}
        }
    }
    let submitted_count = form.choices.len();
    form.submitted_choice_count = submitted_count;
    if submitted_count < 2 {
        form.choices.resize(2, String::new());
    } else if submitted_count > 10 {
        form.choices.truncate(10);
    }
    form
}

#[component]
async fn document(title: String, child: topcoat::view::View) -> Result {
    view! {
        <!DOCTYPE html>
        <html lang="ja">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>(title)</title>
                topcoat::font::link(font: fontsource_font!(GEIST))
                <link rel="stylesheet" href=(tailwind::stylesheet!())>
                topcoat::runtime::script()
                topcoat::dev::script()
            </head>
            <body class="bg-background text-foreground">
                (child)
            </body>
        </html>
    }
}

#[component]
async fn create_form(form: RoomForm) -> Result {
    let public_checked = form.visibility.as_deref() == Some("public");
    let anonymous_checked = form.visibility.as_deref() == Some("anonymous");
    let initial_choice_count = form.choices.len() as f64;
    let mut choices = form.choices.clone();
    choices.resize(10, String::new());
    let question_described_by = if form.question_error.is_some() {
        "question-help question-error"
    } else {
        "question-help"
    };
    let visibility_described_by = if form.visibility_error.is_some() {
        "visibility-help visibility-error"
    } else {
        "visibility-help"
    };
    view! {
        signal choice_count = initial_choice_count;

        <main class="mx-auto min-h-screen max-w-2xl px-6 py-12">
            <h1 class="text-3xl font-semibold">"投票部屋を作る"</h1>
            <p class="mt-2 text-muted-foreground">
                "質問と2〜10個の選択肢を入力してください。"
            </p>
            if !form.errors.is_empty() {
                <div id="form-errors" role="alert" class="mt-6 rounded-lg border border-destructive p-4">
                    <p class="font-medium">"入力内容を確認してください"</p>
                    <ul class="mt-2 list-disc pl-5">
                        for error in &form.errors {
                            <li>(error)</li>
                        }
                    </ul>
                </div>
            }
            <form id="create-room" action="/rooms" method="post" class="mt-8 space-y-7" novalidate=(true)
                @submit=$(|_event: Event| raw!(r#"{
const form = ${_event}.current_target.inner;
form.querySelectorAll('.client-error').forEach(node => node.remove());
form.querySelectorAll('[aria-invalid]').forEach(node => {
  node.removeAttribute('aria-invalid');
  const ids = (node.getAttribute('aria-describedby') || '').split(/\s+/).filter(id => id && !id.endsWith('-error'));
  if (ids.length) node.setAttribute('aria-describedby', ids.join(' '));
  else node.removeAttribute('aria-describedby');
});
const segmenter = new Intl.Segmenter(undefined, { granularity: 'grapheme' });
const lengthOf = value => [...segmenter.segment(value.trim())].length;
let first = null;
const mark = (input, message) => {
  input.setAttribute('aria-invalid', 'true');
  const error = document.createElement('p');
  error.id = input.id + '-error';
  error.className = 'client-error mt-1 text-sm text-destructive';
  error.textContent = message;
  if (input.name === 'visibility') {
    document.querySelector('#visibility-options').insertAdjacentElement('afterend', error);
  } else {
    input.insertAdjacentElement('afterend', error);
  }
  input.setAttribute('aria-describedby', ((input.getAttribute('aria-describedby') || '') + ' ' + error.id).trim());
  first ||= input;
};
const question = form.elements.question;
const questionLength = lengthOf(question.value);
if (questionLength < 1) mark(question, '質問を入力してください。');
else if (questionLength > 200) mark(question, '質問は200文字以内で入力してください。');
else if (/\p{Cc}/u.test(question.value.trim())) mark(question, '質問に制御文字は使えません。');
const inputs = [...form.querySelectorAll('input[name=choice]:not(:disabled)')];
const seen = new Set();
for (const input of inputs) {
  const value = input.value.trim();
  const length = lengthOf(value);
  if (length < 1) mark(input, '選択肢を入力してください。');
  else if (length > 100) mark(input, '選択肢は100文字以内で入力してください。');
  else if (/\p{Cc}/u.test(value)) mark(input, '選択肢に制御文字は使えません。');
  else if (seen.has(value)) mark(input, '選択肢が重複しています。');
  seen.add(value);
}
if (!form.querySelector('input[name=visibility]:checked')) {
  const input = form.querySelector('input[name=visibility]');
  mark(input, '投票者名を公開するか選択してください。');
}
if (first) {
  ${_event}.prevent_default();
  first.focus();
}
}"#))>
                <div>
                    label(
                        attrs: attributes! { for="question" class="block" },
                        "質問"
                    )
                    textarea(
                        attrs: attributes! {
                            id="question"
                            name="question"
                            rows="3"
                            aria-describedby=(question_described_by)
                            aria-invalid=(form.question_error.is_some())
                            class="mt-2"
                        },
                        (form.question)
                    )
                    <p id="question-help" class="mt-1 text-sm text-muted-foreground">"1〜200文字"</p>
                    if let Some(error) = &form.question_error {
                        <p id="question-error" class="mt-1 text-sm text-destructive">(error)</p>
                    }
                </div>
                <fieldset>
                    <legend class="font-medium">"選択肢"</legend>
                    <div id="choices" class="mt-2 space-y-3">
                        for (index, choice) in choices.iter().enumerate() {
                            let row_number = (index + 1) as f64;
                            let choice_error = form.choice_errors.get(index).and_then(Option::as_ref);
                            let described_by = if choice_error.is_some() {
                                format!("choices-help choice-{index}-error")
                            } else {
                                "choices-help".to_string()
                            };
                            <div class="choice-row flex items-start gap-2"
                                :hidden=$(choice_count.get() < row_number)>
                                <div class="min-w-0 flex-1">
                                    label(
                                        attrs: attributes! {
                                            class="sr-only"
                                            for=(format!("choice-{index}"))
                                        },
                                        (format!("選択肢 {}", index + 1))
                                    )
                                    input_component(
                                        attrs: attributes! {
                                            id=(format!("choice-{index}"))
                                            name="choice"
                                            value=(choice)
                                            :disabled=$(choice_count.get() < row_number)
                                            aria-describedby=(described_by)
                                            aria-invalid=(choice_error.is_some())
                                        }
                                    )
                                    if let Some(error) = choice_error {
                                        <p id=(format!("choice-{index}-error"))
                                            class="mt-1 text-sm text-destructive">(error)</p>
                                    }
                                </div>
                                button(
                                    variant: ButtonVariant::Outline,
                                    attrs: attributes! {
                                        type="button"
                                        aria-label=(format!("選択肢 {}を削除", index + 1))
                                        :hidden=$(if choice_count.get() != row_number {
                                            true
                                        } else {
                                            choice_count.get() <= 2.0
                                        })
                                        @click=$(|_event: Event| choice_count.set(choice_count.get() - 1.0))
                                    },
                                    "削除"
                                )
                            </div>
                        }
                    </div>
                    button(
                        variant: ButtonVariant::Outline,
                        attrs: attributes! {
                            id="add-choice"
                            type="button"
                            class="mt-3"
                            :disabled=$(choice_count.get() >= 10.0)
                            @click=$(|_event: Event| choice_count.set(choice_count.get() + 1.0))
                        },
                        "選択肢を追加"
                    )
                    <p id="choices-help" class="mt-1 text-sm text-muted-foreground">
                        "各1〜100文字。同じ選択肢は使えません。"
                    </p>
                </fieldset>
                <fieldset>
                    <legend class="font-medium">"投票者名の公開設定（必須）"</legend>
                    <div id="visibility-options">
                        <label class="mt-2 flex gap-2">
                            <input id="visibility-public" type="radio" name="visibility" value="public" checked=(public_checked)
                                aria-invalid=(form.visibility_error.is_some())
                                aria-describedby=(visibility_described_by)>
                            <span>"公開する（誰がどの選択肢へ投票したかを参加者全員に表示します）"</span>
                        </label>
                        <label class="mt-2 flex gap-2">
                            <input id="visibility-anonymous" type="radio" name="visibility" value="anonymous" checked=(anonymous_checked)
                                aria-invalid=(form.visibility_error.is_some())
                                aria-describedby=(visibility_described_by)>
                            <span>"公開しない（表示名を入力せず匿名で投票します）"</span>
                        </label>
                    </div>
                    if let Some(error) = &form.visibility_error {
                        <p id="visibility-error" class="mt-1 text-sm text-destructive">(error)</p>
                    }
                    <p id="visibility-help" class="mt-1 text-sm text-muted-foreground">
                        "作成後に公開設定は変更できません。"
                    </p>
                </fieldset>
                button(
                    attrs: attributes! { type="submit" },
                    "投票部屋を作成"
                )
            </form>
        </main>
    }
}

#[page("/")]
async fn home() -> Result {
    view! { document(title: "Lite Vote".to_string(), create_form(form: RoomForm::default())) }
}

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

#[path_param]
struct Slug(str);

#[page("/rooms/{slug}")]
async fn room(cx: &Cx) -> Result {
    let slug = path_param::<Slug>(cx);
    let pool = app_context::<SqlitePool>(cx);
    let room: Option<(String, bool, String)> = sqlx::query_as(
        "SELECT question, participant_names_public, creator_token_hash
         FROM voting_rooms WHERE slug = ?",
    )
    .bind(slug)
    .fetch_optional(pool)
    .await
    .map_err(topcoat::Error::from)?;
    let Some((question, names_public, creator_hash)) = room else {
        return view! { (StatusCode::NOT_FOUND) <main><h1>"投票部屋が見つかりません"</h1></main> };
    };
    let choices: Vec<String> =
        sqlx::query_scalar("SELECT text FROM choices WHERE room_id = (SELECT id FROM voting_rooms WHERE slug = ?) ORDER BY position")
            .bind(slug).fetch_all(pool).await.map_err(topcoat::Error::from)?;
    if let Some(cookie) = cookies(cx).get(CREATOR_COOKIE_NAME)
        && hash_token(cookie.value()) == creator_hash
    {
        cookies(cx).add(
            Cookie::build((CREATOR_COOKIE_NAME, cookie.value().to_owned()))
                .path(format!("/rooms/{slug}"))
                .http_only(true)
                .secure(true)
                .same_site(SameSite::Lax)
                .max_age(Duration::seconds(CREATOR_COOKIE_MAX_AGE_SECONDS))
                .build(),
        );
    }
    view! {
        document(title: format!("{question} - Lite Vote"), {
            <main class="mx-auto min-h-screen max-w-2xl px-6 py-12">
                <h1 class="text-3xl font-semibold">(question)</h1>
                <p class="mt-2 text-muted-foreground">
                    if names_public { "投票者名を公開する部屋です。" } else { "匿名の投票部屋です。" }
                </p>
                <ul class="mt-6 space-y-2">
                    for choice in choices { <li class="rounded-lg border p-3">(choice)</li> }
                </ul>
                <p class="mt-8 text-muted-foreground">"投票機能は準備中です。"</p>
            </main>
        })
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

fn error_messages(errors: &[lite_vote::room_creation::CreateRoomValidationError]) -> Vec<String> {
    let mut messages = Vec::new();
    for error in errors {
        match error {
            lite_vote::room_creation::CreateRoomValidationError::Visibility => {
                messages.push("投票者名を公開するか選択してください。".into());
            }
            lite_vote::room_creation::CreateRoomValidationError::Fields(fields) => {
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

fn apply_field_errors(
    form: &mut RoomForm,
    errors: &[lite_vote::room_creation::CreateRoomValidationError],
) {
    for error in errors {
        match error {
            lite_vote::room_creation::CreateRoomValidationError::Visibility => {
                form.visibility_error = Some("投票者名を公開するか選択してください。".to_string());
            }
            lite_vote::room_creation::CreateRoomValidationError::Fields(fields) => {
                for field in fields {
                    let message = error_messages(&[
                        lite_vote::room_creation::CreateRoomValidationError::Fields(vec![
                            field.clone(),
                        ]),
                    ])
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
mod tests {
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
}
