//! Participant lookup and creation for voting-room entry.

use sqlx::{FromRow, SqlitePool};

use crate::{
    models::{Choice, Participant, VotingRoom},
    security::{hash_token, random_token},
    validation::{ValidationError, validate_display_name},
};

pub const PARTICIPANT_COOKIE_NAME: &str = "lite_vote_participant";
pub const PARTICIPANT_COOKIE_MAX_AGE_SECONDS: i64 = 34_560_000;
const MAX_TOKEN_ATTEMPTS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomDetails {
    pub room: VotingRoom,
    pub choices: Vec<Choice>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind<'a> {
    Public(&'a str),
    Anonymous,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryOutcome {
    Created {
        participant: Participant,
        token: String,
    },
    Closed,
    NotFound,
    VisibilityChanged,
}

#[derive(Debug, thiserror::Error)]
pub enum EntryError {
    #[error("invalid display name")]
    Validation(ValidationError),
    #[error("secure random number generation failed")]
    Random,
    #[error("database operation failed")]
    Database(#[from] sqlx::Error),
    #[error("could not allocate a unique participant token")]
    TokenCollision,
}

#[derive(FromRow)]
struct RoomEntryState {
    id: i64,
    participant_names_public: bool,
    closed_at: Option<String>,
}

pub async fn load_room(pool: &SqlitePool, slug: &str) -> Result<Option<RoomDetails>, sqlx::Error> {
    let room: Option<VotingRoom> = sqlx::query_as(
        "SELECT id, slug, question, participant_names_public, creator_token_hash,
                created_at, closed_at
         FROM voting_rooms WHERE slug = ?",
    )
    .bind(slug)
    .fetch_optional(pool)
    .await?;
    let Some(room) = room else {
        return Ok(None);
    };
    let choices = sqlx::query_as(
        "SELECT room_id, id, text, position
         FROM choices WHERE room_id = ? ORDER BY position",
    )
    .bind(room.id)
    .fetch_all(pool)
    .await?;
    Ok(Some(RoomDetails { room, choices }))
}

pub async fn find_participant_by_token(
    pool: &SqlitePool,
    room_id: i64,
    token: &str,
) -> Result<Option<Participant>, sqlx::Error> {
    sqlx::query_as(
        "SELECT room_id, id, token_hash, display_name
         FROM participants WHERE room_id = ? AND token_hash = ?",
    )
    .bind(room_id)
    .bind(hash_token(token))
    .fetch_optional(pool)
    .await
}

pub async fn create_participant(
    pool: &SqlitePool,
    slug: &str,
    kind: EntryKind<'_>,
) -> Result<EntryOutcome, EntryError> {
    create_participant_with_token_source(pool, slug, kind, || {
        random_token().map_err(|_| EntryError::Random)
    })
    .await
}

async fn create_participant_with_token_source(
    pool: &SqlitePool,
    slug: &str,
    kind: EntryKind<'_>,
    mut next_token: impl FnMut() -> Result<String, EntryError>,
) -> Result<EntryOutcome, EntryError> {
    let mut transaction = pool.begin().await?;
    let state: Option<RoomEntryState> = sqlx::query_as(
        "SELECT id, participant_names_public, closed_at
         FROM voting_rooms WHERE slug = ?",
    )
    .bind(slug)
    .fetch_optional(&mut *transaction)
    .await?;
    let Some(state) = state else {
        transaction.rollback().await?;
        return Ok(EntryOutcome::NotFound);
    };
    if state.closed_at.is_some() {
        transaction.rollback().await?;
        return Ok(EntryOutcome::Closed);
    }

    let display_name = match kind {
        EntryKind::Public(display_name) if state.participant_names_public => {
            Some(validate_display_name(display_name).map_err(EntryError::Validation)?)
        }
        EntryKind::Anonymous if !state.participant_names_public => None,
        _ => {
            transaction.rollback().await?;
            return Ok(EntryOutcome::VisibilityChanged);
        }
    };
    let id: i64 =
        sqlx::query_scalar("SELECT COALESCE(MAX(id), 0) + 1 FROM participants WHERE room_id = ?")
            .bind(state.id)
            .fetch_one(&mut *transaction)
            .await?;

    for _ in 0..MAX_TOKEN_ATTEMPTS {
        let token = next_token()?;
        let token_hash = hash_token(&token);
        let inserted = sqlx::query(
            "INSERT INTO participants (room_id, id, token_hash, display_name)
             VALUES (?, ?, ?, ?)",
        )
        .bind(state.id)
        .bind(id)
        .bind(&token_hash)
        .bind(&display_name)
        .execute(&mut *transaction)
        .await;
        match inserted {
            Ok(_) => {
                transaction.commit().await?;
                return Ok(EntryOutcome::Created {
                    participant: Participant {
                        room_id: state.id,
                        id,
                        token_hash,
                        display_name,
                    },
                    token,
                });
            }
            Err(error) if is_token_collision(&error) => continue,
            Err(error) => return Err(error.into()),
        }
    }
    transaction.rollback().await?;
    Err(EntryError::TokenCollision)
}

fn is_token_collision(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|error| error.code())
        .is_some_and(|code| code == "2067")
        && error.to_string().contains("participants.room_id")
        && error.to_string().contains("participants.token_hash")
}

#[cfg(test)]
mod tests;
