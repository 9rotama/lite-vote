//! Transactional voting and vote changes.

use sqlx::{FromRow, SqlitePool};

use crate::security::hash_token;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoteOutcome {
    Recorded,
    Closed,
    RoomNotFound,
    ParticipantNotFound,
    ChoiceNotFound,
}

#[derive(Debug, thiserror::Error)]
pub enum VoteError {
    #[error("database operation failed")]
    Database(#[from] sqlx::Error),
}

#[derive(FromRow)]
struct VotingState {
    room_id: i64,
    closed_at: Option<String>,
}

/// Records a participant's vote, updating the same row when they vote again.
///
/// The room state, participant token, choice, and write are deliberately checked
/// in one transaction so a committed close always prevents a later vote.
pub async fn cast_vote(
    pool: &SqlitePool,
    slug: &str,
    participant_token: &str,
    choice_id: i64,
) -> Result<VoteOutcome, VoteError> {
    // Taking the writer lock before reading the room state serializes the
    // first vote with creator edits, which also start with `BEGIN IMMEDIATE`.
    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
    let state: Option<VotingState> = sqlx::query_as(
        "SELECT id AS room_id, closed_at
         FROM voting_rooms WHERE slug = ?",
    )
    .bind(slug)
    .fetch_optional(&mut *transaction)
    .await?;
    let Some(state) = state else {
        transaction.rollback().await?;
        return Ok(VoteOutcome::RoomNotFound);
    };
    if state.closed_at.is_some() {
        transaction.rollback().await?;
        return Ok(VoteOutcome::Closed);
    }

    let participant_id: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM participants
         WHERE room_id = ? AND token_hash = ?",
    )
    .bind(state.room_id)
    .bind(hash_token(participant_token))
    .fetch_optional(&mut *transaction)
    .await?;
    let Some(participant_id) = participant_id else {
        transaction.rollback().await?;
        return Ok(VoteOutcome::ParticipantNotFound);
    };

    let choice_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM choices WHERE room_id = ? AND id = ?
         )",
    )
    .bind(state.room_id)
    .bind(choice_id)
    .fetch_one(&mut *transaction)
    .await?;
    if !choice_exists {
        transaction.rollback().await?;
        return Ok(VoteOutcome::ChoiceNotFound);
    }

    sqlx::query(
        "INSERT INTO votes (room_id, participant_id, choice_id)
         VALUES (?, ?, ?)
         ON CONFLICT (room_id, participant_id)
         DO UPDATE SET choice_id = excluded.choice_id",
    )
    .bind(state.room_id)
    .bind(participant_id)
    .bind(choice_id)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(VoteOutcome::Recorded)
}

#[cfg(test)]
mod integration_tests;
