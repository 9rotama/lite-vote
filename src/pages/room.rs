use crate::{
    components::{button::button, input::input, label::label},
    pages::layout::document,
};
use lite_vote::{
    participant_entry::{
        EntryKind, EntryOutcome, PARTICIPANT_COOKIE_MAX_AGE_SECONDS, PARTICIPANT_COOKIE_NAME,
        RoomDetails, create_participant, find_participant_by_token, load_room,
    },
    room_creation::{CREATOR_COOKIE_MAX_AGE_SECONDS, CREATOR_COOKIE_NAME},
    security::hash_token,
};
use sqlx::SqlitePool;
use topcoat::{
    Result,
    context::{Cx, app_context},
    cookie::{Cookie, Cookies, SameSite, cookies, time::Duration},
    router::{StatusCode, page, path_param},
    runtime::Event,
    view::{Unescaped, attributes, component, view},
};

#[path_param]
struct Slug(str);

#[derive(Clone, Debug, Default)]
pub(crate) struct ParticipantForm {
    pub(crate) display_name: String,
    pub(crate) error: Option<String>,
}

pub(crate) fn set_participant_cookie(cx: &Cx, slug: &str, token: &str) {
    cookies(cx).add(
        Cookie::build((PARTICIPANT_COOKIE_NAME, token.to_owned()))
            .path(format!("/rooms/{slug}"))
            .http_only(true)
            .secure(true)
            .same_site(SameSite::Lax)
            .max_age(Duration::seconds(PARTICIPANT_COOKIE_MAX_AGE_SECONDS))
            .build(),
    );
}

fn refresh_creator_cookie(cx: &Cx, details: &RoomDetails) {
    if let Some(cookie) = cookies(cx).get(CREATOR_COOKIE_NAME)
        && hash_token(cookie.value()) == details.room.creator_token_hash
    {
        cookies(cx).add(
            Cookie::build((CREATOR_COOKIE_NAME, cookie.value().to_owned()))
                .path(format!("/rooms/{}", details.room.slug))
                .http_only(true)
                .secure(true)
                .same_site(SameSite::Lax)
                .max_age(Duration::seconds(CREATOR_COOKIE_MAX_AGE_SECONDS))
                .build(),
        );
    }
}

pub(crate) fn display_name_error(error: &lite_vote::validation::ValidationError) -> String {
    use lite_vote::validation::ValidationErrorReason;
    match error.reason {
        ValidationErrorReason::Empty => "表示名を入力してください。",
        ValidationErrorReason::ContainsControlCharacter => "表示名に制御文字は使えません。",
        ValidationErrorReason::TooLong { .. } => "表示名は30文字以内で入力してください。",
        ValidationErrorReason::InvalidChoiceCount { .. }
        | ValidationErrorReason::DuplicateChoice { .. } => "表示名を確認してください。",
    }
    .to_owned()
}

#[component]
pub(crate) async fn participant_form(details: RoomDetails, form: ParticipantForm) -> Result {
    let action = format!("/rooms/{}/participants", details.room.slug);
    view! {
        <main class="mx-auto min-h-screen max-w-2xl px-6 py-12">
            <h1 class="text-3xl font-semibold">(details.room.question)</h1>
            <p class="mt-2 text-muted-foreground">"表示名を入力して投票部屋へ入ってください。"</p>
            <div class="mt-6 rounded-lg border p-4">
                <p>
                    "表示名と投票先は参加者全員へ公開され、締切後の結果にも残ります。"
                </p>
            </div>
            <form id="participant-entry" action=(action) method="post" class="mt-8 space-y-6"
                novalidate=(true)
                @submit=$(|_event: Event| raw!(r#"{
const form = ${_event}.current_target.inner;
const input = form.elements.display_name;
const error = document.getElementById('display-name-error');
const value = input.value.trim();
let message = '';
try {
  const segmenter = new Intl.Segmenter(undefined, {granularity: 'grapheme'});
  const length = [...segmenter.segment(value)].length;
  if (length < 1) message = '表示名を入力してください。';
  else if (length > 30) message = '表示名は30文字以内で入力してください。';
  else if (/\p{Cc}/u.test(value)) message = '表示名に制御文字は使えません。';
} catch (_) {
  if (value.length < 1) message = '表示名を入力してください。';
  else if (/\p{Cc}/u.test(value)) message = '表示名に制御文字は使えません。';
}
error.textContent = message;
error.hidden = message === '';
input.setAttribute('aria-invalid', message === '' ? 'false' : 'true');
if (message) {
  ${_event}.prevent_default();
  input.focus();
}
}"#))>
                <div>
                    label(attrs: attributes! { for="display-name" class="block" }, "表示名")
                    input(attrs: attributes! {
                        id="display-name"
                        name="display_name"
                        value=(form.display_name)
                        autocomplete="nickname"
                        aria-describedby="display-name-help display-name-error"
                        aria-invalid=(form.error.is_some())
                        class="mt-2"
                    })
                    <p id="display-name-help" class="mt-1 text-sm text-muted-foreground">
                        "1〜30文字"
                    </p>
                    <p id="display-name-error" class="mt-1 text-sm text-destructive"
                        hidden=(form.error.is_none())>
                        (form.error.unwrap_or_default())
                    </p>
                </div>
                button(attrs: attributes! { type="submit" }, "投票部屋へ入る")
            </form>
            <script>(Unescaped::new_unchecked(r#"try {
  const input = document.getElementById('display-name');
  if (input.value === '') {
    const saved = localStorage.getItem('lite_vote_last_display_name');
    if (saved !== null) input.value = saved;
  }
} catch (_) {}"#))</script>
        </main>
    }
}

#[component]
async fn voting_room(details: RoomDetails, display_name_to_remember: Option<String>) -> Result {
    let closed = details.room.is_closed();
    view! {
        <main class="mx-auto min-h-screen max-w-2xl px-6 py-12">
            <h1 class="text-3xl font-semibold">(details.room.question)</h1>
            <p class="mt-2 text-muted-foreground">
                if closed {
                    "この投票は締め切られています。"
                } else if details.room.participant_names_public {
                    "投票者名を公開する部屋です。"
                } else {
                    "匿名の投票部屋です。"
                }
            </p>
            <ul id="room-choices" class="mt-6 space-y-2">
                for choice in details.choices {
                    <li class="rounded-lg border p-3">(choice.text)</li>
                }
            </ul>
            if !closed {
                <p class="mt-8 text-muted-foreground">"投票機能は準備中です。"</p>
            }
            if let Some(display_name) = display_name_to_remember {
                <script>(Unescaped::new_unchecked(format!(
                    "try {{ localStorage.setItem('lite_vote_last_display_name', {}); }} catch (_) {{}}",
                    javascript_string(&display_name)
                )))</script>
            }
        </main>
    }
}

fn javascript_string(value: &str) -> String {
    let mut encoded = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => encoded.push_str("\\\""),
            '\\' => encoded.push_str("\\\\"),
            '<' => encoded.push_str("\\u003c"),
            '\n' => encoded.push_str("\\n"),
            '\r' => encoded.push_str("\\r"),
            '\u{2028}' => encoded.push_str("\\u2028"),
            '\u{2029}' => encoded.push_str("\\u2029"),
            character => encoded.push(character),
        }
    }
    encoded.push('"');
    encoded
}

#[component]
pub(crate) async fn room_not_found() -> Result {
    view! {
        document(title: "投票部屋が見つかりません - Lite Vote".to_string(), {
            <main class="mx-auto min-h-screen max-w-2xl px-6 py-12">
                <h1 class="text-3xl font-semibold">"投票部屋が見つかりません"</h1>
                <p class="mt-2 text-muted-foreground">
                    "参加用URLを確認してください。"
                </p>
                <a href="/" class="mt-6 inline-block underline">"トップへ戻る"</a>
            </main>
        })
    }
}

#[page("/rooms/{slug}")]
async fn room(cx: &Cx) -> Result {
    let slug = path_param::<Slug>(cx);
    let pool = app_context::<SqlitePool>(cx);
    let Some(details) = load_room(pool, slug).await.map_err(topcoat::Error::from)? else {
        return view! { (StatusCode::NOT_FOUND) room_not_found() };
    };
    refresh_creator_cookie(cx, &details);

    let existing = if let Some(cookie) = cookies(cx).get(PARTICIPANT_COOKIE_NAME) {
        find_participant_by_token(pool, details.room.id, cookie.value())
            .await
            .map_err(topcoat::Error::from)?
            .map(|participant| (participant, cookie.value().to_owned()))
    } else {
        None
    };
    if let Some((participant, token)) = existing {
        set_participant_cookie(cx, slug, &token);
        let remember = details
            .room
            .participant_names_public
            .then_some(participant.display_name)
            .flatten();
        let title = format!("{} - Lite Vote", details.room.question);
        return view! { document(title: title, voting_room(
            details: details,
            display_name_to_remember: remember,
        )) };
    }
    if details.room.is_closed() {
        let title = format!("{} - Lite Vote", details.room.question);
        return view! { document(title: title, voting_room(
            details: details,
            display_name_to_remember: None,
        )) };
    }
    if details.room.participant_names_public {
        let title = format!("{} - Lite Vote", details.room.question);
        return view! { document(title: title, participant_form(
            details: details,
            form: ParticipantForm::default(),
        )) };
    }

    match create_participant(pool, slug, EntryKind::Anonymous).await {
        Ok(EntryOutcome::Created { token, .. }) => {
            set_participant_cookie(cx, slug, &token);
            let title = format!("{} - Lite Vote", details.room.question);
            view! { document(title: title, voting_room(
                details: details,
                display_name_to_remember: None,
            )) }
        }
        Ok(EntryOutcome::Closed | EntryOutcome::VisibilityChanged) => {
            let Some(details) = load_room(pool, slug).await.map_err(topcoat::Error::from)? else {
                return view! { (StatusCode::NOT_FOUND) room_not_found() };
            };
            let title = format!("{} - Lite Vote", details.room.question);
            if details.room.is_closed() {
                view! { document(title: title, voting_room(
                    details: details,
                    display_name_to_remember: None,
                )) }
            } else if details.room.participant_names_public {
                view! { document(title: title, participant_form(
                    details: details,
                    form: ParticipantForm::default(),
                )) }
            } else {
                view! { (StatusCode::INTERNAL_SERVER_ERROR) "internal server error" }
            }
        }
        Ok(EntryOutcome::NotFound) => view! { (StatusCode::NOT_FOUND) room_not_found() },
        Err(_) => view! { (StatusCode::INTERNAL_SERVER_ERROR) "internal server error" },
    }
}

#[cfg(test)]
mod tests {
    use super::javascript_string;

    #[test]
    fn local_storage_value_is_safe_inside_a_script() {
        assert_eq!(javascript_string("A\"\\\n"), "\"A\\\"\\\\\\n\"");
        assert_eq!(
            javascript_string("</script>\u{2028}next"),
            "\"\\u003c/script>\\u2028next\""
        );
    }
}
