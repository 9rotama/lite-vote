use bytes::Bytes;
use http_body_util::Channel;
use lite_vote::{participant_entry::load_room, realtime::RoomUpdateHub};
use sqlx::SqlitePool;
use std::{convert::Infallible, time::Duration};
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{Body, IntoResponse, Response, StatusCode, path_param, route},
};

#[path_param]
struct Slug(str);

#[route(GET "/rooms/{slug}/events")]
async fn get_room_events(cx: &Cx) -> Result<Response> {
    let slug = path_param::<Slug>(cx);
    if load_room(app_context::<SqlitePool>(cx), slug)
        .await
        .map_err(topcoat::Error::from)?
        .is_none()
    {
        return (StatusCode::NOT_FOUND, "room not found").into_response(cx);
    }

    let mut updates = app_context::<RoomUpdateHub>(cx).subscribe();
    let subscribed_slug = slug.to_owned();
    let (mut sender, body) = Channel::<Bytes, Infallible>::new(1);
    tokio::spawn(async move {
        if sender
            .send_data(Bytes::from_static(b": connected\n\n"))
            .await
            .is_err()
        {
            return;
        }

        let mut heartbeat = tokio::time::interval(Duration::from_secs(15));
        heartbeat.tick().await;
        loop {
            let message = tokio::select! {
                update = updates.recv() => match update {
                    Ok(updated_slug) if updated_slug == subscribed_slug => {
                        Some(Bytes::from_static(b"event: update\ndata: changed\n\n"))
                    }
                    Ok(_) => None,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        Some(Bytes::from_static(b"event: update\ndata: resync\n\n"))
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                },
                _ = heartbeat.tick() => {
                    Some(Bytes::from_static(b": heartbeat\n\n"))
                }
            };
            if let Some(message) = message
                && sender.send_data(message).await.is_err()
            {
                break;
            }
        }
    });

    Ok(Response::builder()
        .header("Content-Type", "text/event-stream")
        .header("Cache-Control", "no-cache, no-transform")
        .header("X-Accel-Buffering", "no")
        .body(Body::new(body))?)
}
