use http_body_util::BodyExt;
use lite_vote::{
    db::{DatabaseConfig, MIGRATOR, connect_pool},
    participant_entry::{EntryKind, EntryOutcome, create_participant, load_room},
    realtime::RoomUpdateHub,
    room_creation::{CreateRoomInput, CreatedRoom, create_room},
    voting::cast_vote,
};
use sqlx::SqlitePool;
use std::time::Duration;
use tempfile::tempdir;
use topcoat::{
    asset::{AssetBundle, RouterBuilderAssetExt},
    cookie::RouterBuilderCookieExt,
    router::{Body, Method, Router, RouterBuilderDiscoverExt, StatusCode, header, to_bytes},
};

async fn next_body_chunk(body: &mut Body) -> String {
    let frame = tokio::time::timeout(Duration::from_secs(1), body.frame())
        .await
        .expect("SSE frame should arrive within one second")
        .expect("SSE stream should remain open")
        .expect("SSE frame should be readable");
    String::from_utf8(frame.into_data().expect("expected a data frame").to_vec()).unwrap()
}

async fn app() -> (tempfile::TempDir, SqlitePool, Router) {
    let dir = tempdir().unwrap();
    let pool = connect_pool(&DatabaseConfig {
        path: dir.path().join("http.sqlite3"),
        busy_timeout: Duration::from_secs(5),
    })
    .await
    .unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    let router = Router::builder()
        .discover()
        .assets(AssetBundle::empty())
        .app_context(pool.clone())
        .app_context(RoomUpdateHub::default())
        .cookies()
        .build();
    (dir, pool, router)
}

async fn create(pool: &SqlitePool, visibility: &str) -> String {
    create_owned(pool, visibility).await.slug
}

async fn create_owned(pool: &SqlitePool, visibility: &str) -> CreatedRoom {
    create_room(
        pool,
        &CreateRoomInput {
            question: "次は？".into(),
            choices: vec!["A".into(), "B".into()],
            visibility: Some(visibility.into()),
        },
    )
    .await
    .unwrap()
}

async fn send(
    router: &Router,
    method: Method,
    path: &str,
    body: &str,
    cookie: Option<&str>,
    origin: Option<&str>,
) -> (StatusCode, http::HeaderMap, String) {
    send_with_partial_header(router, method, path, body, cookie, origin, false).await
}

async fn send_with_partial_header(
    router: &Router,
    method: Method,
    path: &str,
    body: &str,
    cookie: Option<&str>,
    origin: Option<&str>,
    partial: bool,
) -> (StatusCode, http::HeaderMap, String) {
    let mut request = http::Request::builder()
        .method(method)
        .uri(path)
        .header(header::HOST, "vote.example");
    if !body.is_empty() {
        request = request.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    }
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    if let Some(origin) = origin {
        request = request.header(header::ORIGIN, origin);
    }
    if partial {
        request = request.header("x-lite-vote-partial", "results");
    }
    let response = router
        .handle(request.body(Body::from(body.to_owned())).unwrap())
        .await;
    let (parts, body) = response.into_parts();
    let body = String::from_utf8(to_bytes(body, usize::MAX).await.unwrap().to_vec()).unwrap();
    (parts.status, parts.headers, body)
}

fn participant_cookie(headers: &http::HeaderMap) -> String {
    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with("lite_vote_participant="))
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}

fn creator_cookie(token: &str) -> String {
    format!("lite_vote_creator={token}")
}

#[tokio::test]
async fn creator_can_open_and_submit_the_room_edit_form() {
    let (_dir, pool, router) = app().await;
    let created = create_owned(&pool, "anonymous").await;
    let cookie = creator_cookie(&created.creator_token);
    let room_path = format!("/rooms/{}", created.slug);
    let edit_path = format!("/rooms/{}/edit", created.slug);

    let (status, _, body) = send(&router, Method::GET, &room_path, "", Some(&cookie), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("締切後は投票を再開できません"));
    assert!(body.contains("aria-describedby=\"close-room-warning\""));

    let (status, _, body) = send(&router, Method::GET, &edit_path, "", Some(&cookie), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("投票部屋を編集"));
    assert!(body.contains("name=\"question\""));
    assert!(body.contains("value=\"A\""));
    assert!(body.contains("選択肢を追加"));

    let (status, headers, _) = send(
        &router,
        Method::POST,
        &edit_path,
        "question=%20Updated%20&choice=%20X%20&choice=Y&choice=Z",
        Some(&cookie),
        Some("https://vote.example"),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        headers.get(header::LOCATION).unwrap().to_str().unwrap(),
        format!("/rooms/{}", created.slug)
    );
    let details = load_room(&pool, &created.slug).await.unwrap().unwrap();
    assert_eq!(details.room.question, "Updated");
    assert_eq!(
        details
            .choices
            .iter()
            .map(|choice| choice.text.as_str())
            .collect::<Vec<_>>(),
        vec!["X", "Y", "Z"]
    );
}

#[tokio::test]
async fn edit_http_endpoint_enforces_creator_validation_and_first_vote() {
    let (_dir, pool, router) = app().await;
    let created = create_owned(&pool, "anonymous").await;
    let cookie = creator_cookie(&created.creator_token);
    let path = format!("/rooms/{}/edit", created.slug);

    let (status, _, _) = send(&router, Method::GET, &path, "", None, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, body) = send(
        &router,
        Method::POST,
        &path,
        "question=&choice=same&choice=same",
        Some(&cookie),
        Some("https://vote.example"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body.contains("質問を入力してください"));
    assert!(body.contains("選択肢 2が重複しています"));

    let participant_token = match create_participant(&pool, &created.slug, EntryKind::Anonymous)
        .await
        .unwrap()
    {
        EntryOutcome::Created { token, .. } => token,
        other => panic!("unexpected entry outcome: {other:?}"),
    };
    let choice_id = load_room(&pool, &created.slug)
        .await
        .unwrap()
        .unwrap()
        .choices[0]
        .id;
    cast_vote(&pool, &created.slug, &participant_token, choice_id)
        .await
        .unwrap();

    let (status, _, _) = send(&router, Method::GET, &path, "", Some(&cookie), None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _, body) = send(
        &router,
        Method::POST,
        &path,
        "question=Too+late&choice=X&choice=Y",
        Some(&cookie),
        Some("https://vote.example"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body.contains("編集できません"));
}

#[tokio::test]
async fn public_room_requires_a_valid_display_name_before_showing_choices() {
    let (_dir, pool, router) = app().await;
    let slug = create(&pool, "public").await;
    let path = format!("/rooms/{slug}");
    let (status, headers, body) = send(&router, Method::GET, &path, "", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(headers.get_all(header::SET_COOKIE).iter().next().is_none());
    assert!(body.contains("次は？"));
    assert!(body.contains("name=\"display_name\""));
    assert!(body.contains("for=\"display-name\""));
    assert!(body.contains("aria-describedby=\"display-name-help display-name-error\""));
    assert!(body.contains("締切後の結果にも残ります"));
    assert!(!body.contains("id=\"room-choices\""));

    let post_path = format!("{path}/participants");
    let (status, headers, body) = send(
        &router,
        Method::POST,
        &post_path,
        "display_name=%20",
        None,
        Some("https://vote.example"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body.contains("表示名を入力してください"));
    assert!(headers.get_all(header::SET_COOKIE).iter().next().is_none());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM participants")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn public_entry_sets_a_scoped_secure_cookie_and_revisit_reuses_participant() {
    let (_dir, pool, router) = app().await;
    let slug = create(&pool, "public").await;
    let post_path = format!("/rooms/{slug}/participants");
    let (status, headers, body) = send(
        &router,
        Method::POST,
        &post_path,
        "display_name=%20Alice%20",
        None,
        Some("https://vote.example"),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        headers.get(header::LOCATION).unwrap().to_str().unwrap(),
        format!("/rooms/{slug}")
    );
    assert!(body.is_empty());
    let set_cookie = headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with("lite_vote_participant="))
        .unwrap();
    assert!(set_cookie.contains("HttpOnly"));
    assert!(set_cookie.contains("Secure"));
    assert!(set_cookie.contains("SameSite=Lax"));
    assert!(set_cookie.contains(&format!("Path=/rooms/{slug}")));
    assert!(set_cookie.contains("Max-Age=34560000"));
    let cookie = participant_cookie(&headers);
    let token = cookie.split_once('=').unwrap().1;
    let stored: (String, String) =
        sqlx::query_as("SELECT display_name, token_hash FROM participants")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored.0, "Alice");
    assert_ne!(stored.1, token);
    assert!(!body.contains(token));

    let path = format!("/rooms/{slug}");
    let (status, headers, body) = send(&router, Method::GET, &path, "", Some(&cookie), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("id=\"room-choices\""));
    assert!(body.contains("現在の結果"));
    assert!(body.contains("lite_vote_last_display_name"));
    assert!(
        headers
            .get_all(header::SET_COOKIE)
            .iter()
            .any(|value| value.to_str().unwrap().contains("Max-Age=34560000"))
    );
    assert!(!body.contains(token));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM participants")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn anonymous_room_creates_participant_on_get_without_using_display_name_storage() {
    let (_dir, pool, router) = app().await;
    let slug = create(&pool, "anonymous").await;
    let path = format!("/rooms/{slug}");
    let (status, headers, body) = send(&router, Method::GET, &path, "", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("id=\"room-choices\""));
    assert!(body.contains("現在の結果"));
    assert!(!body.contains("name=\"display_name\""));
    assert!(!body.contains("lite_vote_last_display_name"));
    let _cookie = participant_cookie(&headers);
    let display_name: Option<String> = sqlx::query_scalar("SELECT display_name FROM participants")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(display_name, None);
}

#[tokio::test]
async fn closed_missing_and_cross_origin_requests_have_no_participant_side_effects() {
    let (_dir, pool, router) = app().await;
    let slug = create(&pool, "public").await;
    sqlx::query("UPDATE voting_rooms SET closed_at = CURRENT_TIMESTAMP WHERE slug = ?")
        .bind(&slug)
        .execute(&pool)
        .await
        .unwrap();
    let path = format!("/rooms/{slug}");
    let (status, headers, body) = send(&router, Method::GET, &path, "", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("id=\"room-results\""));
    assert!(body.contains("確定結果"));
    assert!(body.contains("勝者なし"));
    assert!(body.contains("締め切られています"));
    assert!(headers.get_all(header::SET_COOKIE).iter().next().is_none());

    let post_path = format!("{path}/participants");
    let (status, _, _) = send(
        &router,
        Method::POST,
        &post_path,
        "display_name=Alice",
        None,
        Some("https://vote.example"),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let (status, _, _) = send(
        &router,
        Method::POST,
        &post_path,
        "display_name=Alice",
        None,
        Some("https://evil.example"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, body) = send(&router, Method::GET, "/rooms/missing", "", None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains("<html"));
    let (status, _, _) = send(
        &router,
        Method::POST,
        "/rooms/missing/participants",
        "display_name=Alice",
        None,
        Some("https://vote.example"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM participants")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn vote_route_uses_radios_and_updates_the_existing_participant_vote() {
    let (_dir, pool, router) = app().await;
    let slug = create(&pool, "anonymous").await;
    let room_path = format!("/rooms/{slug}");
    let (status, headers, body) = send(&router, Method::GET, &room_path, "", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.matches("type=\"radio\"").count(), 2);
    assert_eq!(body.matches("name=\"choice_id\"").count(), 2);
    assert!(body.contains("投票先を一つ選んでください"));
    assert_eq!(body.matches(">選択中</span>").count(), 2);
    assert!(body.contains("peer-checked:inline"));
    let cookie = participant_cookie(&headers);
    let vote_path = format!("{room_path}/votes");

    let (status, headers, _) = send(
        &router,
        Method::POST,
        &vote_path,
        "choice_id=1",
        Some(&cookie),
        Some("https://vote.example"),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(headers.get(header::LOCATION).unwrap(), &room_path);

    let (status, _, _) = send(
        &router,
        Method::POST,
        &vote_path,
        "choice_id=2",
        Some(&cookie),
        Some("https://vote.example"),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let votes: Vec<i64> = sqlx::query_scalar("SELECT choice_id FROM votes")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(votes, vec![2]);

    let (status, _, body) = send(&router, Method::GET, &room_path, "", Some(&cookie), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("投票先を変更する"));
    assert!(body.contains("value=\"2\" checked"));
}

#[tokio::test]
async fn partial_vote_response_updates_only_results_and_announces_the_change() {
    let (_dir, pool, router) = app().await;
    let slug = create(&pool, "anonymous").await;
    let room_path = format!("/rooms/{slug}");
    let (status, headers, body) = send(&router, Method::GET, &room_path, "", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("X-Lite-Vote-Partial"));
    assert!(body.contains("id=\"results-update-status\""));
    assert!(body.contains("role=\"status\""));
    let cookie = participant_cookie(&headers);

    let (status, _, body) = send_with_partial_header(
        &router,
        Method::POST,
        &format!("{room_path}/votes"),
        "choice_id=1",
        Some(&cookie),
        Some("https://vote.example"),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.starts_with("<section"));
    assert!(body.contains("id=\"room-results\""));
    assert!(body.contains("data-total-votes=\"1\""));
    assert!(body.contains("1票（100.0%）"));
    assert!(!body.contains("<html"));
    assert!(!body.contains("id=\"room-choices\""));
}

#[tokio::test]
async fn results_fragment_obeys_room_name_visibility() {
    let (_dir, pool, router) = app().await;
    let public_slug = create(&pool, "public").await;
    let public_token = match create_participant(&pool, &public_slug, EntryKind::Public("Alice"))
        .await
        .unwrap()
    {
        EntryOutcome::Created { token, .. } => token,
        other => panic!("unexpected entry outcome: {other:?}"),
    };
    let public_choice = load_room(&pool, &public_slug)
        .await
        .unwrap()
        .unwrap()
        .choices[0]
        .id;
    cast_vote(&pool, &public_slug, &public_token, public_choice)
        .await
        .unwrap();
    let (status, _, body) = send(
        &router,
        Method::GET,
        &format!("/rooms/{public_slug}/results"),
        "",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Alice"));
    assert!(body.contains("投票者:"));
    assert!(body.contains("1票（100.0%）"));

    let anonymous_slug = create(&pool, "anonymous").await;
    let anonymous_token = match create_participant(&pool, &anonymous_slug, EntryKind::Anonymous)
        .await
        .unwrap()
    {
        EntryOutcome::Created { token, .. } => token,
        other => panic!("unexpected entry outcome: {other:?}"),
    };
    let anonymous_details = load_room(&pool, &anonymous_slug).await.unwrap().unwrap();
    sqlx::query("UPDATE participants SET display_name = 'Secret name' WHERE room_id = ?")
        .bind(anonymous_details.room.id)
        .execute(&pool)
        .await
        .unwrap();
    cast_vote(
        &pool,
        &anonymous_slug,
        &anonymous_token,
        anonymous_details.choices[1].id,
    )
    .await
    .unwrap();
    let (status, _, body) = send(
        &router,
        Method::GET,
        &format!("/rooms/{anonymous_slug}/results"),
        "",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("1票（100.0%）"));
    assert!(!body.contains("Secret name"));
    assert!(!body.contains("投票者:"));
}

#[tokio::test]
async fn vote_route_rejects_missing_or_invalid_security_state_and_closed_rooms() {
    let (_dir, pool, router) = app().await;
    let slug = create(&pool, "anonymous").await;
    let room_path = format!("/rooms/{slug}");
    let (_, headers, _) = send(&router, Method::GET, &room_path, "", None, None).await;
    let cookie = participant_cookie(&headers);
    let vote_path = format!("{room_path}/votes");

    let (status, _, _) = send(
        &router,
        Method::POST,
        &vote_path,
        "choice_id=1",
        Some(&cookie),
        Some("https://evil.example"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, _) = send(
        &router,
        Method::POST,
        &vote_path,
        "choice_id=1",
        None,
        Some("https://vote.example"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, _) = send(
        &router,
        Method::POST,
        &vote_path,
        "choice_id=999",
        Some(&cookie),
        Some("https://vote.example"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    sqlx::query("UPDATE voting_rooms SET closed_at = CURRENT_TIMESTAMP WHERE slug = ?")
        .bind(&slug)
        .execute(&pool)
        .await
        .unwrap();
    let (status, _, body) = send(
        &router,
        Method::POST,
        &vote_path,
        "choice_id=1",
        Some(&cookie),
        Some("https://vote.example"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body.contains("締め切られています"));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM votes")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn creator_closes_once_and_participant_url_shows_all_tied_winners() {
    let (_dir, pool, router) = app().await;
    let created = create_owned(&pool, "anonymous").await;
    let room_path = format!("/rooms/{}", created.slug);
    let close_path = format!("{room_path}/close");

    let (_, first_headers, _) = send(&router, Method::GET, &room_path, "", None, None).await;
    let first_cookie = participant_cookie(&first_headers);
    let (_, second_headers, _) = send(&router, Method::GET, &room_path, "", None, None).await;
    let second_cookie = participant_cookie(&second_headers);
    let vote_path = format!("{room_path}/votes");
    for (cookie, choice_id) in [(&first_cookie, 1), (&second_cookie, 2)] {
        let (status, _, _) = send(
            &router,
            Method::POST,
            &vote_path,
            &format!("choice_id={choice_id}"),
            Some(cookie),
            Some("https://vote.example"),
        )
        .await;
        assert_eq!(status, StatusCode::SEE_OTHER);
    }

    let creator_cookie = creator_cookie(&created.creator_token);
    let (status, headers, _) = send(
        &router,
        Method::POST,
        &close_path,
        "",
        Some(&creator_cookie),
        Some("https://vote.example"),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(headers.get(header::LOCATION).unwrap(), &room_path);
    let closed_at: String = sqlx::query_scalar("SELECT closed_at FROM voting_rooms WHERE slug = ?")
        .bind(&created.slug)
        .fetch_one(&pool)
        .await
        .unwrap();

    let (status, _, _) = send(
        &router,
        Method::POST,
        &close_path,
        "",
        Some(&creator_cookie),
        Some("https://vote.example"),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT closed_at FROM voting_rooms WHERE slug = ?")
            .bind(&created.slug)
            .fetch_one(&pool)
            .await
            .unwrap(),
        closed_at
    );

    let (status, headers, body) = send(&router, Method::GET, &room_path, "", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(headers.get_all(header::SET_COOKIE).iter().next().is_none());
    assert!(body.contains("確定結果"));
    assert!(body.contains("合計 2票"));
    assert_eq!(body.matches("1票（50.0%）").count(), 2);
    assert_eq!(body.matches("最多得票").count(), 2);
    assert!(!body.contains("type=\"radio\""));
    assert!(!body.contains("投票を締め切る"));
    assert!(!body.contains("質問と選択肢を編集"));
}

#[tokio::test]
async fn close_route_requires_creator_and_same_origin() {
    let (_dir, pool, router) = app().await;
    let created = create_owned(&pool, "anonymous").await;
    let close_path = format!("/rooms/{}/close", created.slug);
    let cookie = creator_cookie(&created.creator_token);

    let (status, _, _) = send(
        &router,
        Method::POST,
        &close_path,
        "",
        None,
        Some("https://vote.example"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, _) = send(
        &router,
        Method::POST,
        &close_path,
        "",
        Some(&cookie),
        Some("https://evil.example"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(
        load_room(&pool, &created.slug)
            .await
            .unwrap()
            .unwrap()
            .room
            .closed_at
            .is_none()
    );
}

#[tokio::test]
async fn room_event_stream_notifies_votes_and_closing() {
    let (_dir, pool, router) = app().await;
    let created = create_owned(&pool, "anonymous").await;
    let room_path = format!("/rooms/{}", created.slug);

    let (status, headers, body) = send(&router, Method::GET, &room_path, "", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("new EventSource"));
    assert!(body.contains("リアルタイム更新に接続中"));
    let room_participant_cookie = participant_cookie(&headers);

    let event_request = http::Request::builder()
        .method(Method::GET)
        .uri(format!("{room_path}/events"))
        .header(header::HOST, "vote.example")
        .body(Body::empty())
        .unwrap();
    let response = router.handle(event_request).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/event-stream"
    );
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-cache, no-transform"
    );
    assert_eq!(response.headers().get("x-accel-buffering").unwrap(), "no");
    let mut event_body = response.into_body();
    assert_eq!(next_body_chunk(&mut event_body).await, ": connected\n\n");

    let other = create_owned(&pool, "anonymous").await;
    let other_room_path = format!("/rooms/{}", other.slug);
    let (status, headers, _) = send(&router, Method::GET, &other_room_path, "", None, None).await;
    assert_eq!(status, StatusCode::OK);
    let other_participant_cookie = participant_cookie(&headers);
    let other_choice_id = load_room(&pool, &other.slug)
        .await
        .unwrap()
        .unwrap()
        .choices[0]
        .id;
    let (status, _, _) = send(
        &router,
        Method::POST,
        &format!("{other_room_path}/votes"),
        &format!("choice_id={other_choice_id}"),
        Some(&other_participant_cookie),
        Some("https://vote.example"),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), event_body.frame())
            .await
            .is_err(),
        "another room's update must not be sent on this stream"
    );

    let choice_id = load_room(&pool, &created.slug)
        .await
        .unwrap()
        .unwrap()
        .choices[0]
        .id;
    let (status, _, _) = send(
        &router,
        Method::POST,
        &format!("{room_path}/votes"),
        &format!("choice_id={choice_id}"),
        Some(&room_participant_cookie),
        Some("https://vote.example"),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        next_body_chunk(&mut event_body).await,
        "event: update\ndata: changed\n\n"
    );

    let (status, _, _) = send(
        &router,
        Method::POST,
        &format!("{room_path}/close"),
        "",
        Some(&creator_cookie(&created.creator_token)),
        Some("https://vote.example"),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        next_body_chunk(&mut event_body).await,
        "event: update\ndata: changed\n\n"
    );
}

#[tokio::test]
async fn event_stream_returns_not_found_for_unknown_room() {
    let (_dir, _pool, router) = app().await;
    let (status, _, body) = send(
        &router,
        Method::GET,
        "/rooms/not-found/events",
        "",
        None,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, "room not found");
}
