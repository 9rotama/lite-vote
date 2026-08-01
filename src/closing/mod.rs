//! Creator-authorized, irreversible voting-room closing.

use sqlx::{FromRow, SqlitePool};

use crate::security::hash_token;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseRoomOutcome {
    Closed,
    NotFound,
    Forbidden,
}

#[derive(Debug, thiserror::Error)]
pub enum CloseRoomError {
    #[error("database operation failed")]
    Database(#[from] sqlx::Error),
}

#[derive(FromRow)]
struct CloseState {
    creator_token_hash: String,
    closed_at: Option<String>,
}

/// Closes a room once and treats repeated authorized closes as success.
///
/// `BEGIN IMMEDIATE` serializes this operation with voting. Whichever
/// transaction commits first determines whether the competing vote is part of
/// the final result.
pub async fn close_room(
    pool: &SqlitePool,
    slug: &str,
    creator_token: &str,
) -> Result<CloseRoomOutcome, CloseRoomError> {
    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
    let state: Option<CloseState> = sqlx::query_as(
        "SELECT creator_token_hash, closed_at
         FROM voting_rooms WHERE slug = ?",
    )
    .bind(slug)
    .fetch_optional(&mut *transaction)
    .await?;
    let Some(state) = state else {
        transaction.rollback().await?;
        return Ok(CloseRoomOutcome::NotFound);
    };
    if state.creator_token_hash != hash_token(creator_token) {
        transaction.rollback().await?;
        return Ok(CloseRoomOutcome::Forbidden);
    }
    if state.closed_at.is_none() {
        sqlx::query(
            "UPDATE voting_rooms
             SET closed_at = CURRENT_TIMESTAMP
             WHERE slug = ? AND closed_at IS NULL",
        )
        .bind(slug)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    Ok(CloseRoomOutcome::Closed)
}

#[cfg(test)]
mod integration_tests;
