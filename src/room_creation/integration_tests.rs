use crate::{
    db::{DatabaseConfig, MIGRATOR, connect_pool},
    room_creation::{CreateRoomInput, create_room, hash_token},
};
use std::time::Duration;
use tempfile::tempdir;

async fn database() -> (tempfile::TempDir, sqlx::SqlitePool) {
    let dir = tempdir().unwrap();
    let pool = connect_pool(&DatabaseConfig {
        path: dir.path().join("test.sqlite3"),
        busy_timeout: Duration::from_secs(5),
    })
    .await
    .unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    (dir, pool)
}

#[tokio::test]
async fn creates_public_room_and_preserves_choice_order() {
    let (_dir, pool) = database().await;
    let choices: Vec<_> = (0..10).map(|index| format!(" choice {index} ")).collect();
    let created = create_room(
        &pool,
        &CreateRoomInput {
            question: " question ".into(),
            choices,
            visibility: Some("public".into()),
        },
    )
    .await
    .unwrap();

    let room: (String, bool, String) = sqlx::query_as(
        "SELECT question, participant_names_public, creator_token_hash
         FROM voting_rooms WHERE id = ?",
    )
    .bind(created.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(room.0, "question");
    assert!(room.1);
    assert_eq!(room.2, hash_token(&created.creator_token));
    assert_ne!(room.2, created.creator_token);
    let persisted: Vec<(String, i64)> =
        sqlx::query_as("SELECT text, position FROM choices WHERE room_id = ? ORDER BY position")
            .bind(created.id)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        persisted,
        (0..10)
            .map(|index| (format!("choice {index}"), index))
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn invalid_input_has_no_database_side_effects() {
    let (_dir, pool) = database().await;
    let error = create_room(
        &pool,
        &CreateRoomInput {
            question: " ".into(),
            choices: vec!["same".into(), "same".into()],
            visibility: Some("invalid".into()),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(
        error,
        crate::room_creation::CreateRoomError::Validation(_)
    ));
    let rooms: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM voting_rooms")
        .fetch_one(&pool)
        .await
        .unwrap();
    let choices: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM choices")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!((rooms, choices), (0, 0));
}

#[tokio::test]
async fn choice_failure_rolls_back_the_room() {
    let (_dir, pool) = database().await;
    let trigger = "CREATE TRIGGER reject_choice BEFORE INSERT ON choices
                   BEGIN SELECT RAISE(FAIL, 'rejected for test'); END";
    sqlx::query(trigger).execute(&pool).await.unwrap();
    let result = create_room(
        &pool,
        &CreateRoomInput {
            question: "Q".into(),
            choices: vec!["A".into(), "B".into()],
            visibility: Some("anonymous".into()),
        },
    )
    .await;
    assert!(result.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM voting_rooms")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}
