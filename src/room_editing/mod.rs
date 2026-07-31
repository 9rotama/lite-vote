//! Creator-authorized room editing before the first vote.

use sqlx::{FromRow, SqlitePool};

use crate::{
    security::hash_token,
    validation::{ValidatedVotingRoom, ValidationError, validate_voting_room},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditRoomInput {
    pub question: String,
    pub choices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditRoomOutcome {
    Updated,
    NotFound,
    Forbidden,
    VotingStarted,
    Closed,
}

#[derive(Debug, thiserror::Error)]
pub enum EditRoomError {
    #[error("invalid room input")]
    Validation(Vec<ValidationError>),
    #[error("database operation failed")]
    Database(#[from] sqlx::Error),
}

#[derive(FromRow)]
struct RejectedEditState {
    creator_token_hash: String,
    closed_at: Option<String>,
    vote_count: i64,
}

/// Updates the question and replaces all choices if this creator still owns an
/// open room and no vote has been recorded.
///
/// The conditional `UPDATE` is deliberately the first database operation in
/// the transaction. It acquires SQLite's writer lock while checking for the
/// first vote, so an edit and the first vote cannot both commit against the
/// same pre-vote state.
pub async fn edit_room(
    pool: &SqlitePool,
    slug: &str,
    creator_token: &str,
    input: &EditRoomInput,
) -> Result<EditRoomOutcome, EditRoomError> {
    let ValidatedVotingRoom { question, choices } =
        validate_voting_room(&input.question, &input.choices).map_err(EditRoomError::Validation)?;
    let creator_token_hash = hash_token(creator_token);
    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;

    let updated = sqlx::query(
        "UPDATE voting_rooms
         SET question = ?
         WHERE slug = ?
           AND creator_token_hash = ?
           AND closed_at IS NULL
           AND NOT EXISTS (
               SELECT 1 FROM votes WHERE votes.room_id = voting_rooms.id
           )",
    )
    .bind(&question)
    .bind(slug)
    .bind(&creator_token_hash)
    .execute(&mut *transaction)
    .await?;

    if updated.rows_affected() == 0 {
        let state: Option<RejectedEditState> = sqlx::query_as(
            "SELECT creator_token_hash, closed_at,
                    (SELECT COUNT(*) FROM votes WHERE room_id = voting_rooms.id) AS vote_count
             FROM voting_rooms WHERE slug = ?",
        )
        .bind(slug)
        .fetch_optional(&mut *transaction)
        .await?;
        transaction.rollback().await?;
        return Ok(match state {
            None => EditRoomOutcome::NotFound,
            Some(state) if state.creator_token_hash != creator_token_hash => {
                EditRoomOutcome::Forbidden
            }
            Some(state) if state.closed_at.is_some() => EditRoomOutcome::Closed,
            Some(state) if state.vote_count > 0 => EditRoomOutcome::VotingStarted,
            Some(_) => EditRoomOutcome::Forbidden,
        });
    }

    let room_id: i64 = sqlx::query_scalar("SELECT id FROM voting_rooms WHERE slug = ?")
        .bind(slug)
        .fetch_one(&mut *transaction)
        .await?;
    let last_choice_id: i64 =
        sqlx::query_scalar("SELECT COALESCE(MAX(id), 0) FROM choices WHERE room_id = ?")
            .bind(room_id)
            .fetch_one(&mut *transaction)
            .await?;
    sqlx::query("DELETE FROM choices WHERE room_id = ?")
        .bind(room_id)
        .execute(&mut *transaction)
        .await?;
    for (position, text) in choices.iter().enumerate() {
        sqlx::query(
            "INSERT INTO choices (room_id, id, text, position)
             VALUES (?, ?, ?, ?)",
        )
        .bind(room_id)
        .bind(last_choice_id + position as i64 + 1)
        .bind(text)
        .bind(position as i64)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    Ok(EditRoomOutcome::Updated)
}

pub async fn room_has_votes(pool: &SqlitePool, room_id: i64) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM votes WHERE room_id = ?)")
        .bind(room_id)
        .fetch_one(pool)
        .await
}

#[cfg(test)]
mod integration_tests;
