use crate::pages::room::{results_region, room_not_found};
use lite_vote::{participant_entry::load_room, results::load_results};
use sqlx::SqlitePool;
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{StatusCode, path_param, route},
    view::view,
};

#[path_param]
struct Slug(str);

#[route(GET "/rooms/{slug}/results")]
async fn get_results(cx: &Cx) -> Result {
    let slug = path_param::<Slug>(cx);
    let pool = app_context::<SqlitePool>(cx);
    let Some(details) = load_room(pool, slug).await.map_err(topcoat::Error::from)? else {
        return view! { (StatusCode::NOT_FOUND) room_not_found() };
    };
    let closed = details.room.is_closed();
    let results = load_results(
        pool,
        details.room.id,
        closed,
        details.room.participant_names_public,
    )
    .await
    .map_err(topcoat::Error::from)?;

    view! {
        results_region(
            results: results,
            closed: closed,
            participant_names_public: details.room.participant_names_public,
        )
    }
}
