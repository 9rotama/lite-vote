use super::*;
use crate::{
    db::{DatabaseConfig, MIGRATOR, connect_pool},
    room_creation::{CreateRoomInput, create_room},
};
use std::time::Duration;
use tempfile::tempdir;

async fn database() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempdir().unwrap();
    let pool = connect_pool(&DatabaseConfig {
        path: dir.path().join("participants.sqlite3"),
        busy_timeout: Duration::from_secs(5),
    })
    .await
    .unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    (dir, pool)
}

async fn room(pool: &SqlitePool, visibility: &str) -> String {
    create_room(
        pool,
        &CreateRoomInput {
            question: "どれにする？".into(),
            choices: vec!["A".into(), "B".into()],
            visibility: Some(visibility.into()),
        },
    )
    .await
    .unwrap()
    .slug
}

#[tokio::test]
async fn public_and_anonymous_participants_store_the_expected_names() {
    let (_dir, pool) = database().await;
    let public = room(&pool, "public").await;
    let anonymous = room(&pool, "anonymous").await;
    let EntryOutcome::Created {
        participant: named,
        token: named_token,
    } = create_participant(&pool, &public, EntryKind::Public("  Alice  "))
        .await
        .unwrap()
    else {
        panic!("participant should be created");
    };
    let EntryOutcome::Created {
        participant: unnamed,
        token: anonymous_token,
    } = create_participant(&pool, &anonymous, EntryKind::Anonymous)
        .await
        .unwrap()
    else {
        panic!("participant should be created");
    };

    assert_eq!(named.display_name.as_deref(), Some("Alice"));
    assert_eq!(unnamed.display_name, None);
    assert_ne!(named.token_hash, named_token);
    assert_ne!(unnamed.token_hash, anonymous_token);
    let persisted: Vec<(Option<String>, String)> =
        sqlx::query_as("SELECT display_name, token_hash FROM participants ORDER BY room_id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(
        !persisted
            .iter()
            .any(|(_, hash)| { hash == &named_token || hash == &anonymous_token })
    );
}

#[tokio::test]
async fn duplicate_display_names_are_distinct_participants_found_only_by_token() {
    let (_dir, pool) = database().await;
    let slug = room(&pool, "public").await;
    let first = create_participant(&pool, &slug, EntryKind::Public("same"))
        .await
        .unwrap();
    let second = create_participant(&pool, &slug, EntryKind::Public("same"))
        .await
        .unwrap();
    let (
        EntryOutcome::Created {
            participant: first_participant,
            token: first_token,
        },
        EntryOutcome::Created {
            participant: second_participant,
            token: second_token,
        },
    ) = (first, second)
    else {
        panic!("participants should be created");
    };

    assert_ne!(first_participant.id, second_participant.id);
    assert_ne!(first_participant.token_hash, second_participant.token_hash);
    assert_eq!(
        find_participant_by_token(&pool, first_participant.room_id, &first_token)
            .await
            .unwrap()
            .unwrap()
            .id,
        first_participant.id
    );
    assert_eq!(
        find_participant_by_token(&pool, first_participant.room_id, &second_token)
            .await
            .unwrap()
            .unwrap()
            .id,
        second_participant.id
    );
    assert!(
        find_participant_by_token(&pool, first_participant.room_id, "tampered")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn participant_tokens_are_scoped_to_the_room() {
    let (_dir, pool) = database().await;
    let first_slug = room(&pool, "anonymous").await;
    let second_slug = room(&pool, "anonymous").await;
    let EntryOutcome::Created { participant, token } =
        create_participant(&pool, &first_slug, EntryKind::Anonymous)
            .await
            .unwrap()
    else {
        panic!("participant should be created");
    };
    let second_room = load_room(&pool, &second_slug).await.unwrap().unwrap();
    assert!(
        find_participant_by_token(&pool, second_room.room.id, &token)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        find_participant_by_token(&pool, participant.room_id, &token)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn closed_and_missing_rooms_do_not_create_participants() {
    let (_dir, pool) = database().await;
    let slug = room(&pool, "public").await;
    sqlx::query("UPDATE voting_rooms SET closed_at = CURRENT_TIMESTAMP WHERE slug = ?")
        .bind(&slug)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        create_participant(&pool, &slug, EntryKind::Public("Alice"))
            .await
            .unwrap(),
        EntryOutcome::Closed
    );
    assert_eq!(
        create_participant(&pool, "missing", EntryKind::Anonymous)
            .await
            .unwrap(),
        EntryOutcome::NotFound
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM participants")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn token_collision_is_retried_without_leaving_partial_data() {
    let (_dir, pool) = database().await;
    let slug = room(&pool, "anonymous").await;
    let first = create_participant_with_token_source(&pool, &slug, EntryKind::Anonymous, || {
        Ok("collision".into())
    })
    .await
    .unwrap();
    assert!(matches!(first, EntryOutcome::Created { .. }));
    let mut tokens = ["collision", "available"].into_iter();
    let EntryOutcome::Created { token, .. } =
        create_participant_with_token_source(&pool, &slug, EntryKind::Anonymous, || {
            Ok(tokens.next().unwrap().into())
        })
        .await
        .unwrap()
    else {
        panic!("participant should be created after retry");
    };
    assert_eq!(token, "available");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM participants")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );

    let error = create_participant_with_token_source(&pool, &slug, EntryKind::Anonymous, || {
        Ok("collision".into())
    })
    .await
    .unwrap_err();
    assert!(matches!(error, EntryError::TokenCollision));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM participants")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
}

#[tokio::test]
async fn invalid_display_name_and_visibility_mismatch_have_no_side_effects() {
    let (_dir, pool) = database().await;
    let public = room(&pool, "public").await;
    let anonymous = room(&pool, "anonymous").await;
    assert!(matches!(
        create_participant(&pool, &public, EntryKind::Public(" "))
            .await
            .unwrap_err(),
        EntryError::Validation(_)
    ));
    assert_eq!(
        create_participant(&pool, &public, EntryKind::Anonymous)
            .await
            .unwrap(),
        EntryOutcome::VisibilityChanged
    );
    assert_eq!(
        create_participant(&pool, &anonymous, EntryKind::Public("Alice"))
            .await
            .unwrap(),
        EntryOutcome::VisibilityChanged
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM participants")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}
