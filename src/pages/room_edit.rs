use crate::{
    components::{
        button::{ButtonVariant, button},
        input::input,
        label::label,
        textarea::textarea,
    },
    pages::{layout::document, room::room_not_found},
};
use lite_vote::{
    participant_entry::load_room, room_creation::CREATOR_COOKIE_NAME, room_editing::room_has_votes,
    security::hash_token,
};
use sqlx::SqlitePool;
use topcoat::{
    Result,
    context::{Cx, app_context},
    cookie::{Cookies, cookies},
    router::{StatusCode, page, path_param},
    runtime::Event,
    view::{attributes, component, view},
};

#[path_param]
struct Slug(str);

#[derive(Clone, Debug)]
pub(crate) struct EditRoomForm {
    pub(crate) question: String,
    pub(crate) choices: Vec<String>,
    pub(crate) submitted_choice_count: usize,
    pub(crate) errors: Vec<String>,
    pub(crate) question_error: Option<String>,
    pub(crate) choice_errors: Vec<Option<String>>,
}

impl EditRoomForm {
    fn from_details(details: &lite_vote::participant_entry::RoomDetails) -> Self {
        Self {
            question: details.room.question.clone(),
            choices: details
                .choices
                .iter()
                .map(|choice| choice.text.clone())
                .collect(),
            submitted_choice_count: details.choices.len(),
            errors: Vec::new(),
            question_error: None,
            choice_errors: vec![None; 10],
        }
    }
}

pub(crate) fn edit_form_from_pairs(pairs: Vec<(String, String)>) -> EditRoomForm {
    let mut form = EditRoomForm {
        question: String::new(),
        choices: Vec::new(),
        submitted_choice_count: 0,
        errors: Vec::new(),
        question_error: None,
        choice_errors: vec![None; 10],
    };
    for (name, value) in pairs {
        match name.as_str() {
            "question" => form.question = value,
            "choice" => form.choices.push(value),
            _ => {}
        }
    }
    form.submitted_choice_count = form.choices.len();
    if form.choices.len() < 2 {
        form.choices.resize(2, String::new());
    } else if form.choices.len() > 10 {
        form.choices.truncate(10);
    }
    form
}

#[component]
pub(crate) async fn edit_room_form(slug: String, form: EditRoomForm) -> Result {
    let action = format!("/rooms/{slug}/edit");
    let initial_choice_count = form.choices.len() as f64;
    let mut choices = form.choices.clone();
    choices.resize(10, String::new());
    view! {
        signal choice_count = initial_choice_count;
        <main class="mx-auto min-h-screen max-w-2xl px-4 py-8 sm:px-6 sm:py-12">
            <h1 class="text-2xl font-semibold sm:text-3xl">"投票部屋を編集"</h1>
            <p class="mt-2 text-muted-foreground">
                "質問と選択肢は、最初の一票が入るまで編集できます。"
            </p>
            if !form.errors.is_empty() {
                <div id="form-errors" role="alert"
                    class="mt-6 rounded-lg border border-destructive p-4">
                    <p class="font-medium">"入力内容を確認してください"</p>
                    <ul class="mt-2 list-disc pl-5">
                        for error in &form.errors {
                            <li>(error)</li>
                        }
                    </ul>
                </div>
            }
            <form id="edit-room" action=(action) method="post" class="mt-8 space-y-7"
                novalidate=(true)
                @submit=$(|_event: Event| raw!(r#"{
const form = ${_event}.current_target.inner;
const segmenter = new Intl.Segmenter(undefined, {granularity: 'grapheme'});
const lengthOf = value => [...segmenter.segment(value.trim())].length;
let first = null;
const setError = (input, error, message) => {
  error.textContent = message;
  error.hidden = message === '';
  input.setAttribute('aria-invalid', message === '' ? 'false' : 'true');
  if (message && first === null) first = input;
};
const question = form.elements.question;
let message = '';
const questionLength = lengthOf(question.value);
if (questionLength < 1) message = '質問を入力してください。';
else if (questionLength > 200) message = '質問は200文字以内で入力してください。';
else if (/\p{Cc}/u.test(question.value.trim())) message = '質問に制御文字は使えません。';
setError(question, document.getElementById('question-error'), message);
const inputs = [...form.querySelectorAll('input[name=choice]:not(:disabled)')];
const seen = new Set();
for (const input of inputs) {
  const value = input.value.trim();
  const length = lengthOf(value);
  message = '';
  if (length < 1) message = '選択肢を入力してください。';
  else if (length > 100) message = '選択肢は100文字以内で入力してください。';
  else if (/\p{Cc}/u.test(value)) message = '選択肢に制御文字は使えません。';
  else if (seen.has(value)) message = '選択肢が重複しています。';
  seen.add(value);
  setError(input, document.getElementById(input.id + '-error'), message);
}
if (first !== null) {
  ${_event}.prevent_default();
  first.focus();
}
}"#))>
                <div>
                    label(attrs: attributes! { for="question" class="block" }, "質問")
                    textarea(
                        attrs: attributes! {
                            id="question"
                            name="question"
                            rows="3"
                            aria-describedby="question-help question-error"
                            aria-invalid=(form.question_error.is_some())
                            class="mt-2"
                        },
                        (form.question)
                    )
                    <p id="question-help" class="mt-1 text-sm text-muted-foreground">
                        "1〜200文字"
                    </p>
                    <p id="question-error" class="mt-1 text-sm text-destructive"
                        hidden=(form.question_error.is_none())>
                        (form.question_error.clone().unwrap_or_default())
                    </p>
                </div>
                <fieldset>
                    <legend class="font-medium">"選択肢"</legend>
                    <div id="choices" class="mt-2 space-y-3">
                        for (index, choice) in choices.iter().enumerate() {
                            let row_number = (index + 1) as f64;
                            let choice_error = form.choice_errors.get(index).and_then(Option::as_ref);
                            <div class="choice-row flex flex-col items-stretch gap-2 sm:flex-row sm:items-start"
                                :hidden=$(choice_count.get() < row_number)>
                                <div class="min-w-0 flex-1">
                                    label(
                                        attrs: attributes! {
                                            class="sr-only"
                                            for=(format!("choice-{index}"))
                                        },
                                        (format!("選択肢 {}", index + 1))
                                    )
                                    input(attrs: attributes! {
                                        id=(format!("choice-{index}"))
                                        name="choice"
                                        value=(choice)
                                        :disabled=$(choice_count.get() < row_number)
                                        aria-describedby=(format!("choices-help choice-{index}-error"))
                                        aria-invalid=(choice_error.is_some())
                                    })
                                    <p id=(format!("choice-{index}-error"))
                                        class="mt-1 text-sm text-destructive"
                                        hidden=(choice_error.is_none())>
                                        (choice_error.cloned().unwrap_or_default())
                                    </p>
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
                        "2〜10個、各1〜100文字。同じ選択肢は使えません。"
                    </p>
                </fieldset>
                <div class="flex flex-wrap items-center gap-3">
                    button(attrs: attributes! { type="submit" }, "変更を保存")
                    <a href=(format!("/rooms/{slug}")) class="self-center underline">
                        "キャンセル"
                    </a>
                </div>
            </form>
        </main>
    }
}

#[page("/rooms/{slug}/edit")]
async fn room_edit(cx: &Cx) -> Result {
    let slug = path_param::<Slug>(cx);
    let pool = app_context::<SqlitePool>(cx);
    let Some(details) = load_room(pool, slug).await.map_err(topcoat::Error::from)? else {
        return view! { (StatusCode::NOT_FOUND) room_not_found() };
    };
    let creator = cookies(cx)
        .get(CREATOR_COOKIE_NAME)
        .is_some_and(|cookie| hash_token(cookie.value()) == details.room.creator_token_hash);
    if !creator {
        return view! { (StatusCode::FORBIDDEN) "creator cookie required" };
    }
    if details.room.is_closed() || room_has_votes(pool, details.room.id).await? {
        return view! {
            (StatusCode::CONFLICT)
            document(title: "編集できません - Lite Vote".to_string(), {
                <main class="mx-auto min-h-screen max-w-2xl px-4 py-8 sm:px-6 sm:py-12">
                    <h1 class="text-2xl font-semibold sm:text-3xl">"投票部屋を編集できません"</h1>
                    <p class="mt-2">"締切済み、または最初の一票が入ったため編集できません。"</p>
                    <a href=(format!("/rooms/{slug}")) class="mt-6 inline-block underline">
                        "投票部屋へ戻る"
                    </a>
                </main>
            })
        };
    }
    let title = format!("編集 - {}", details.room.question);
    view! { document(
        title: title,
        edit_room_form(slug: slug.to_owned(), form: EditRoomForm::from_details(&details))
    ) }
}
