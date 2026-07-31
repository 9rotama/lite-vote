use super::*;
use crate::{
    db::{DatabaseConfig, MIGRATOR, connect_pool},
    participant_entry::{EntryKind, EntryOutcome, create_participant, load_room},
    room_creation::{CreateRoomInput, create_room},
};
use std::time::Duration;
use tempfile::tempdir;

async fn database() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempdir().unwrap();
    let pool = connect_pool(&DatabaseConfig {
        path: dir.path().join("voting.sqlite3"),
        busy_timeout: Duration::from_secs(5),
    })
    .await
    .unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    (dir, pool)
}

async fn room(pool: &SqlitePool, visibility: &str) -> (String, String, i64, i64) {
    let created = create_room(
        pool,
        &CreateRoomInput {
            question: "どれにする？".into(),
            choices: vec!["A".into(), "B".into()],
            visibility: Some(visibility.into()),
        },
    )
    .await
    .unwrap();
    let details = load_room(pool, &created.slug).await.unwrap().unwrap();
    (
        created.slug,
        created.creator_token,
        details.choices[0].id,
        details.choices[1].id,
    )
}

async fn participant(pool: &SqlitePool, slug: &str, kind: EntryKind<'_>) -> String {
    let EntryOutcome::Created { token, .. } = create_participant(pool, slug, kind).await.unwrap()
    else {
        panic!("participant should be created");
    };
    token
}

#[tokio::test]
async fn repeat_vote_updates_one_record_for_public_and_anonymous_participants() {
    let (_dir, pool) = database().await;
    for visibility in ["public", "anonymous"] {
        let (slug, _, first_choice, second_choice) = room(&pool, visibility).await;
        let token = if visibility == "public" {
            participant(&pool, &slug, EntryKind::Public("Alice")).await
        } else {
            participant(&pool, &slug, EntryKind::Anonymous).await
        };

        assert_eq!(
            cast_vote(&pool, &slug, &token, first_choice).await.unwrap(),
            VoteOutcome::Recorded
        );
        assert_eq!(
            cast_vote(&pool, &slug, &token, second_choice)
                .await
                .unwrap(),
            VoteOutcome::Recorded
        );
        let details = load_room(&pool, &slug).await.unwrap().unwrap();
        let votes: Vec<i64> = sqlx::query_scalar("SELECT choice_id FROM votes WHERE room_id = ?")
            .bind(details.room.id)
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(votes, vec![second_choice]);
    }
}

#[tokio::test]
async fn creator_votes_as_an_independent_normal_participant() {
    let (_dir, pool) = database().await;
    let (slug, creator_token, choice_id, _) = room(&pool, "anonymous").await;
    let participant_token = participant(&pool, &slug, EntryKind::Anonymous).await;

    assert_eq!(
        cast_vote(&pool, &slug, &creator_token, choice_id)
            .await
            .unwrap(),
        VoteOutcome::ParticipantNotFound
    );
    assert_eq!(
        cast_vote(&pool, &slug, &participant_token, choice_id)
            .await
            .unwrap(),
        VoteOutcome::Recorded
    );
}

#[tokio::test]
async fn closed_room_invalid_participant_and_foreign_choice_are_rejected_without_writes() {
    let (_dir, pool) = database().await;
    let (slug, _, first_choice, _) = room(&pool, "anonymous").await;
    let token = participant(&pool, &slug, EntryKind::Anonymous).await;
    assert_eq!(
        cast_vote(&pool, &slug, "tampered", first_choice)
            .await
            .unwrap(),
        VoteOutcome::ParticipantNotFound
    );
    assert_eq!(
        cast_vote(&pool, &slug, &token, 999).await.unwrap(),
        VoteOutcome::ChoiceNotFound
    );
    sqlx::query("UPDATE voting_rooms SET closed_at = CURRENT_TIMESTAMP WHERE slug = ?")
        .bind(&slug)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        cast_vote(&pool, &slug, &token, first_choice).await.unwrap(),
        VoteOutcome::Closed
    );
    assert_eq!(
        cast_vote(&pool, "missing", &token, first_choice)
            .await
            .unwrap(),
        VoteOutcome::RoomNotFound
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM votes")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}
