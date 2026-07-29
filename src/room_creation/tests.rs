use super::*;
use crate::db::{DatabaseConfig, MIGRATOR, connect_pool};
use std::time::Duration;
use tempfile::tempdir;

#[test]
fn visibility_accepts_only_the_two_protocol_values() {
    assert_eq!(Visibility::parse("public"), Some(Visibility::Public));
    assert_eq!(Visibility::parse("anonymous"), Some(Visibility::Anonymous));
    for invalid in ["", "PUBLIC", "private", " public"] {
        assert_eq!(Visibility::parse(invalid), None);
    }
}

#[tokio::test]
async fn retries_a_slug_collision_without_leaving_a_partial_room() {
    let dir = tempdir().unwrap();
    let pool = connect_pool(&DatabaseConfig {
        path: dir.path().join("collision.sqlite3"),
        busy_timeout: Duration::from_secs(5),
    })
    .await
    .unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO voting_rooms (
            slug, question, participant_names_public, creator_token_hash, created_at
         ) VALUES ('occupied', 'existing', 0, 'existing-hash', CURRENT_TIMESTAMP)",
    )
    .execute(&pool)
    .await
    .unwrap();

    let mut tokens = ["creator-token", "occupied", "available"].into_iter();
    let created = create_room_with_token_source(
        &pool,
        &CreateRoomInput {
            question: "new room".into(),
            choices: vec!["first".into(), "second".into()],
            visibility: Some("public".into()),
        },
        || Ok(tokens.next().unwrap().to_owned()),
    )
    .await
    .unwrap();

    assert_eq!(created.slug, "available");
    assert_eq!(created.creator_token, "creator-token");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM voting_rooms")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM choices
             WHERE room_id = (SELECT id FROM voting_rooms WHERE slug = 'available')",
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        2
    );
}
