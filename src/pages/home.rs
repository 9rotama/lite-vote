use crate::components::button::{ButtonVariant, button};
use crate::components::{input::input as input_component, label::label, textarea::textarea};
use crate::pages::layout::document;
use topcoat::{
    Result,
    router::page,
    runtime::Event,
    view::{attributes, component, view},
};

#[derive(Clone, Debug)]
pub(crate) struct RoomForm {
    pub(crate) question: String,
    pub(crate) choices: Vec<String>,
    pub(crate) visibility: Option<String>,
    pub(crate) errors: Vec<String>,
    pub(crate) submitted_choice_count: usize,
    pub(crate) question_error: Option<String>,
    pub(crate) choice_errors: Vec<Option<String>>,
    pub(crate) visibility_error: Option<String>,
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

pub(crate) fn form_from_pairs(pairs: Vec<(String, String)>) -> RoomForm {
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
pub(crate) async fn create_form(form: RoomForm) -> Result {
    let public_checked = form.visibility.as_deref() == Some("public");
    let anonymous_checked = form.visibility.as_deref() == Some("anonymous");
    let initial_choice_count = form.choices.len() as f64;
    let mut choices = form.choices.clone();
    choices.resize(10, String::new());
    view! {
        signal choice_count = initial_choice_count;

        <main class="mx-auto min-h-screen max-w-2xl px-4 py-8 sm:px-6 sm:py-12">
            <h1 class="text-2xl font-semibold sm:text-3xl">"投票部屋を作る"</h1>
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
let questionMessage = '';
const questionLength = lengthOf(question.value);
if (questionLength < 1) questionMessage = '質問を入力してください。';
else if (questionLength > 200) questionMessage = '質問は200文字以内で入力してください。';
else if (/\p{Cc}/u.test(question.value.trim())) questionMessage = '質問に制御文字は使えません。';
setError(question, document.getElementById('question-error'), questionMessage);

const inputs = [...form.querySelectorAll('input[name=choice]:not(:disabled)')];
const seen = new Set();
for (const input of inputs) {
  const value = input.value.trim();
  const length = lengthOf(value);
  let message = '';
  if (length < 1) message = '選択肢を入力してください。';
  else if (length > 100) message = '選択肢は100文字以内で入力してください。';
  else if (/\p{Cc}/u.test(value)) message = '選択肢に制御文字は使えません。';
  else if (seen.has(value)) message = '選択肢が重複しています。';
  seen.add(value);
  setError(input, document.getElementById(input.id + '-error'), message);
}

const visibilityInputs = [...form.querySelectorAll('input[name=visibility]')];
const visibilityError = document.getElementById('visibility-error');
const visibilityMessage = visibilityInputs.some(input => input.checked)
  ? ''
  : '投票者名を公開するか選択してください。';
visibilityError.textContent = visibilityMessage;
visibilityError.hidden = visibilityMessage === '';
for (const input of visibilityInputs) {
  input.setAttribute('aria-invalid', visibilityMessage === '' ? 'false' : 'true');
}
if (visibilityMessage && first === null) first = visibilityInputs[0];

if (first !== null) {
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
                            aria-describedby="question-help question-error"
                            aria-invalid=(form.question_error.is_some())
                            class="mt-2"
                        },
                        (form.question)
                    )
                    <p id="question-help" class="mt-1 text-sm text-muted-foreground">"1〜200文字"</p>
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
                            let described_by = format!("choices-help choice-{index}-error");
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
                        "各1〜100文字。同じ選択肢は使えません。"
                    </p>
                </fieldset>
                <fieldset>
                    <legend class="font-medium">"投票者名の公開設定（必須）"</legend>
                    <div id="visibility-options" class="space-y-2">
                        <label class="mt-2 flex cursor-pointer items-start gap-3 rounded-lg border p-3
                            focus-within:ring-2 focus-within:ring-ring focus-within:ring-offset-2
                            focus-within:ring-offset-background">
                        <input id="visibility-public" type="radio" name="visibility" value="public" checked=(public_checked)
                            aria-invalid=(form.visibility_error.is_some())
                            aria-describedby="visibility-help visibility-error"
                            class="mt-0.5 size-4 shrink-0">
                            <span>"公開する（誰がどの選択肢へ投票したかを参加者全員に表示します）"</span>
                        </label>
                        <label class="flex cursor-pointer items-start gap-3 rounded-lg border p-3
                            focus-within:ring-2 focus-within:ring-ring focus-within:ring-offset-2
                            focus-within:ring-offset-background">
                        <input id="visibility-anonymous" type="radio" name="visibility" value="anonymous" checked=(anonymous_checked)
                            aria-invalid=(form.visibility_error.is_some())
                            aria-describedby="visibility-help visibility-error"
                            class="mt-0.5 size-4 shrink-0">
                            <span>"公開しない（表示名を入力せず匿名で投票します）"</span>
                        </label>
                    </div>
                    <p id="visibility-error" class="mt-1 text-sm text-destructive"
                        hidden=(form.visibility_error.is_none())>
                        (form.visibility_error.clone().unwrap_or_default())
                    </p>
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
