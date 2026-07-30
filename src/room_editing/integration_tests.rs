use super::{EditRoomInput, EditRoomOutcome, edit_room};
use crate::{
    db::{DatabaseConfig, MIGRATOR, connect_pool},
    participant_entry::{EntryKind, create_participant, load_room},
    room_creation::{CreateRoomInput, create_room},
    voting::{VoteOutcome, cast_vote},
};
use std::time::Duration;
use tempfile::tempdir;

async fn setup() -> (tempfile::TempDir, sqlx::SqlitePool, String, String) {
    let dir = tempdir().unwrap();
    let pool = connect_pool(&DatabaseConfig {
        path: dir.path().join("room-editing.sqlite3"),
        busy_timeout: Duration::from_secs(5),
    })
    .await
    .unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    let created = create_room(
        &pool,
        &CreateRoomInput {
            question: "Before".into(),
            choices: vec!["A".into(), "B".into()],
            visibility: Some("anonymous".into()),
        },
    )
    .await
    .unwrap();
    (dir, pool, created.slug, created.creator_token)
}

fn edit_input() -> EditRoomInput {
    EditRoomInput {
        question: " After ".into(),
        choices: vec![" X ".into(), "Y".into(), "Z".into()],
    }
}

#[tokio::test]
async fn creator_can_edit_question_and_choices_before_any_vote() {
    let (_dir, pool, slug, creator_token) = setup().await;
    assert_eq!(
        edit_room(&pool, &slug, &creator_token, &edit_input())
            .await
            .unwrap(),
        EditRoomOutcome::Updated
    );
    let details = load_room(&pool, &slug).await.unwrap().unwrap();
    assert_eq!(details.room.question, "After");
    assert_eq!(
        details
            .choices
            .iter()
            .map(|choice| (choice.id, choice.text.as_str(), choice.position))
            .collect::<Vec<_>>(),
        vec![(3, "X", 0), (4, "Y", 1), (5, "Z", 2)]
    );
}

#[tokio::test]
async fn invalid_and_non_creator_edits_have_no_side_effects() {
    let (_dir, pool, slug, creator_token) = setup().await;
    let invalid = EditRoomInput {
        question: " ".into(),
        choices: vec!["same".into(), "same".into()],
    };
    assert!(
        edit_room(&pool, &slug, &creator_token, &invalid)
            .await
            .is_err()
    );
    assert_eq!(
        edit_room(&pool, &slug, "not-the-creator", &edit_input())
            .await
            .unwrap(),
        EditRoomOutcome::Forbidden
    );
    let details = load_room(&pool, &slug).await.unwrap().unwrap();
    assert_eq!(details.room.question, "Before");
    assert_eq!(details.choices[0].text, "A");
}

#[tokio::test]
async fn first_vote_and_closed_room_are_rejected_by_the_server() {
    let (_dir, pool, slug, creator_token) = setup().await;
    let entry = create_participant(&pool, &slug, EntryKind::Anonymous)
        .await
        .unwrap();
    let participant_token = match entry {
        crate::participant_entry::EntryOutcome::Created { token, .. } => token,
        other => panic!("unexpected entry outcome: {other:?}"),
    };
    let choice_id = load_room(&pool, &slug).await.unwrap().unwrap().choices[0].id;
    assert_eq!(
        cast_vote(&pool, &slug, &participant_token, choice_id)
            .await
            .unwrap(),
        VoteOutcome::Recorded
    );
    assert_eq!(
        edit_room(&pool, &slug, &creator_token, &edit_input())
            .await
            .unwrap(),
        EditRoomOutcome::VotingStarted
    );

    sqlx::query("DELETE FROM votes")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE voting_rooms SET closed_at = CURRENT_TIMESTAMP WHERE slug = ?")
        .bind(&slug)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        edit_room(&pool, &slug, &creator_token, &edit_input())
            .await
            .unwrap(),
        EditRoomOutcome::Closed
    );
}

#[tokio::test]
async fn edit_and_first_vote_are_serialized_across_sqlite_connections() {
    let (dir, pool, slug, creator_token) = setup().await;
    let entry = create_participant(&pool, &slug, EntryKind::Anonymous)
        .await
        .unwrap();
    let participant_token = match entry {
        crate::participant_entry::EntryOutcome::Created { token, .. } => token,
        other => panic!("unexpected entry outcome: {other:?}"),
    };
    let choice_id = load_room(&pool, &slug).await.unwrap().unwrap().choices[0].id;
    let second_pool = connect_pool(&DatabaseConfig {
        path: dir.path().join("room-editing.sqlite3"),
        busy_timeout: Duration::from_secs(5),
    })
    .await
    .unwrap();

    let input = edit_input();
    let (edit, vote) = tokio::join!(
        edit_room(&pool, &slug, &creator_token, &input),
        cast_vote(&second_pool, &slug, &participant_token, choice_id),
    );
    let edit = edit.unwrap();
    let vote = vote.unwrap();
    assert!(matches!(
        edit,
        EditRoomOutcome::Updated | EditRoomOutcome::VotingStarted
    ));

    let details = load_room(&pool, &slug).await.unwrap().unwrap();
    match edit {
        EditRoomOutcome::Updated => {
            assert_eq!(vote, VoteOutcome::ChoiceNotFound);
            assert_eq!(details.room.question, "After");
        }
        EditRoomOutcome::VotingStarted => {
            assert_eq!(vote, VoteOutcome::Recorded);
            assert_eq!(details.room.question, "Before");
        }
        other => panic!("unexpected edit outcome: {other:?}"),
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM votes")
            .fetch_one(&pool)
            .await
            .unwrap(),
        i64::from(vote == VoteOutcome::Recorded)
    );
}
