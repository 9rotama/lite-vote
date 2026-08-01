use super::*;
use crate::{
    db::{DatabaseConfig, MIGRATOR, connect_pool},
    participant_entry::{EntryKind, EntryOutcome, create_participant, load_room},
    results::load_results,
    room_creation::{CreateRoomInput, create_room},
    voting::{VoteOutcome, cast_vote},
};
use std::time::Duration;
use tempfile::tempdir;

async fn database() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempdir().unwrap();
    let pool = connect_pool(&DatabaseConfig {
        path: dir.path().join("closing.sqlite3"),
        busy_timeout: Duration::from_secs(5),
    })
    .await
    .unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    (dir, pool)
}

async fn room(pool: &SqlitePool) -> (String, String, Vec<i64>) {
    let created = create_room(
        pool,
        &CreateRoomInput {
            question: "どれにする？".into(),
            choices: vec!["A".into(), "B".into(), "C".into()],
            visibility: Some("anonymous".into()),
        },
    )
    .await
    .unwrap();
    let choices = load_room(pool, &created.slug)
        .await
        .unwrap()
        .unwrap()
        .choices
        .into_iter()
        .map(|choice| choice.id)
        .collect();
    (created.slug, created.creator_token, choices)
}

async fn participant(pool: &SqlitePool, slug: &str) -> String {
    let EntryOutcome::Created { token, .. } = create_participant(pool, slug, EntryKind::Anonymous)
        .await
        .unwrap()
    else {
        panic!("participant should be created");
    };
    token
}

#[tokio::test]
async fn zero_vote_close_is_authorized_irreversible_and_idempotent() {
    let (_dir, pool) = database().await;
    let (slug, creator_token, _) = room(&pool).await;

    assert_eq!(
        close_room(&pool, &slug, "tampered").await.unwrap(),
        CloseRoomOutcome::Forbidden
    );
    assert_eq!(
        close_room(&pool, "missing", &creator_token).await.unwrap(),
        CloseRoomOutcome::NotFound
    );
    assert_eq!(
        close_room(&pool, &slug, &creator_token).await.unwrap(),
        CloseRoomOutcome::Closed
    );
    let first_closed_at: String =
        sqlx::query_scalar("SELECT closed_at FROM voting_rooms WHERE slug = ?")
            .bind(&slug)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        close_room(&pool, &slug, &creator_token).await.unwrap(),
        CloseRoomOutcome::Closed
    );
    let second_closed_at: String =
        sqlx::query_scalar("SELECT closed_at FROM voting_rooms WHERE slug = ?")
            .bind(&slug)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(first_closed_at, second_closed_at);

    let details = load_room(&pool, &slug).await.unwrap().unwrap();
    let results = load_results(
        &pool,
        details.room.id,
        details.room.is_closed(),
        details.room.participant_names_public,
    )
    .await
    .unwrap();
    assert_eq!(results.total_votes, 0);
    assert!(results.choices.iter().all(|choice| !choice.is_winner));
}

#[tokio::test]
async fn votes_committed_before_close_are_final_and_later_changes_are_rejected() {
    let (_dir, pool) = database().await;
    let (slug, creator_token, choices) = room(&pool).await;
    let first = participant(&pool, &slug).await;
    let second = participant(&pool, &slug).await;
    assert_eq!(
        cast_vote(&pool, &slug, &first, choices[0]).await.unwrap(),
        VoteOutcome::Recorded
    );
    assert_eq!(
        cast_vote(&pool, &slug, &second, choices[1]).await.unwrap(),
        VoteOutcome::Recorded
    );
    assert_eq!(
        close_room(&pool, &slug, &creator_token).await.unwrap(),
        CloseRoomOutcome::Closed
    );
    assert_eq!(
        cast_vote(&pool, &slug, &first, choices[2]).await.unwrap(),
        VoteOutcome::Closed
    );

    let details = load_room(&pool, &slug).await.unwrap().unwrap();
    let results = load_results(&pool, details.room.id, true, false)
        .await
        .unwrap();
    assert_eq!(results.total_votes, 2);
    assert!(results.choices[0].is_winner);
    assert!(results.choices[1].is_winner);
    assert!(!results.choices[2].is_winner);
}

#[tokio::test]
async fn concurrent_vote_and_close_commit_one_consistent_final_result() {
    let dir = tempdir().unwrap();
    let config = DatabaseConfig {
        path: dir.path().join("closing-race.sqlite3"),
        busy_timeout: Duration::from_secs(5),
    };
    let closing_pool = connect_pool(&config).await.unwrap();
    MIGRATOR.run(&closing_pool).await.unwrap();
    let voting_pool = connect_pool(&config).await.unwrap();

    for _ in 0..20 {
        let (slug, creator_token, choices) = room(&closing_pool).await;
        let participant_token = participant(&closing_pool, &slug).await;
        let (close, vote) = tokio::join!(
            close_room(&closing_pool, &slug, &creator_token),
            cast_vote(&voting_pool, &slug, &participant_token, choices[0]),
        );
        assert_eq!(close.unwrap(), CloseRoomOutcome::Closed);

        let details = load_room(&closing_pool, &slug).await.unwrap().unwrap();
        let results = load_results(&closing_pool, details.room.id, true, false)
            .await
            .unwrap();
        match vote.unwrap() {
            VoteOutcome::Recorded => assert_eq!(results.total_votes, 1),
            VoteOutcome::Closed => assert_eq!(results.total_votes, 0),
            outcome => panic!("unexpected vote outcome: {outcome:?}"),
        }
    }
}

#[tokio::test]
async fn public_results_keep_voter_names_while_anonymous_results_do_not_expose_them() {
    let (_dir, pool) = database().await;
    let created = create_room(
        &pool,
        &CreateRoomInput {
            question: "どれにする？".into(),
            choices: vec!["A".into(), "B".into()],
            visibility: Some("public".into()),
        },
    )
    .await
    .unwrap();
    let choice_id = load_room(&pool, &created.slug)
        .await
        .unwrap()
        .unwrap()
        .choices[0]
        .id;
    let EntryOutcome::Created {
        token: alice_token, ..
    } = create_participant(&pool, &created.slug, EntryKind::Public("Alice"))
        .await
        .unwrap()
    else {
        panic!("Alice should be created");
    };
    let EntryOutcome::Created {
        token: bob_token, ..
    } = create_participant(&pool, &created.slug, EntryKind::Public("Bob"))
        .await
        .unwrap()
    else {
        panic!("Bob should be created");
    };
    cast_vote(&pool, &created.slug, &alice_token, choice_id)
        .await
        .unwrap();
    cast_vote(&pool, &created.slug, &bob_token, choice_id)
        .await
        .unwrap();
    close_room(&pool, &created.slug, &created.creator_token)
        .await
        .unwrap();

    let details = load_room(&pool, &created.slug).await.unwrap().unwrap();
    let public_results = load_results(&pool, details.room.id, true, true)
        .await
        .unwrap();
    assert_eq!(public_results.choices[0].voter_names, vec!["Alice", "Bob"]);
    let anonymous_results = load_results(&pool, details.room.id, true, false)
        .await
        .unwrap();
    assert!(
        anonymous_results
            .choices
            .iter()
            .all(|choice| choice.voter_names.is_empty())
    );
}
