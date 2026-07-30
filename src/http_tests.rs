use lite_vote::{
    db::{DatabaseConfig, MIGRATOR, connect_pool},
    room_creation::{CreateRoomInput, create_room},
};
use sqlx::SqlitePool;
use std::time::Duration;
use tempfile::tempdir;
use topcoat::{
    asset::{AssetBundle, RouterBuilderAssetExt},
    cookie::RouterBuilderCookieExt,
    router::{Body, Method, Router, RouterBuilderDiscoverExt, StatusCode, header, to_bytes},
};

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
        .cookies()
        .build();
    (dir, pool, router)
}

async fn create(pool: &SqlitePool, visibility: &str) -> String {
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
    .slug
}

async fn send(
    router: &Router,
    method: Method,
    path: &str,
    body: &str,
    cookie: Option<&str>,
    origin: Option<&str>,
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
    assert!(body.contains("id=\"room-choices\""));
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
