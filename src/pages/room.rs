use crate::pages::layout::document;
use lite_vote::room_creation::{CREATOR_COOKIE_MAX_AGE_SECONDS, CREATOR_COOKIE_NAME, hash_token};
use sqlx::SqlitePool;
use topcoat::{
    Result,
    context::{Cx, app_context},
    cookie::{Cookie, Cookies, SameSite, cookies, time::Duration},
    router::{StatusCode, page, path_param},
    view::view,
};

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
