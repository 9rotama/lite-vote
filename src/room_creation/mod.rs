//! Voting room creation.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::TryRngCore;
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::validation::{ValidatedVotingRoom, ValidationError, validate_voting_room};

pub const TOKEN_BYTES: usize = 32;
pub const ENCODED_TOKEN_LENGTH: usize = 43;
pub const CREATOR_COOKIE_NAME: &str = "lite_vote_creator";
pub const CREATOR_COOKIE_MAX_AGE_SECONDS: i64 = 34_560_000;
const MAX_SLUG_ATTEMPTS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Anonymous,
}

impl Visibility {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "public" => Some(Self::Public),
            "anonymous" => Some(Self::Anonymous),
            _ => None,
        }
    }

    pub fn names_public(self) -> bool {
        matches!(self, Self::Public)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateRoomInput {
    pub question: String,
    pub choices: Vec<String>,
    pub visibility: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateRoomValidationError {
    Fields(Vec<ValidationError>),
    Visibility,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedRoom {
    pub id: i64,
    pub slug: String,
    pub creator_token: String,
}

#[derive(Debug, thiserror::Error)]
pub enum CreateRoomError {
    #[error("invalid room input")]
    Validation(Vec<CreateRoomValidationError>),
    #[error("secure random number generation failed")]
    Random,
    #[error("database operation failed")]
    Database(#[from] sqlx::Error),
    #[error("could not allocate a unique room URL")]
    SlugCollision,
}

pub fn random_token() -> Result<String, CreateRoomError> {
    let mut bytes = [0_u8; TOKEN_BYTES];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|_| CreateRoomError::Random)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

pub fn hash_token(token: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(token.as_bytes()))
}

pub fn validate_create_room(
    input: &CreateRoomInput,
) -> Result<(ValidatedVotingRoom, Visibility), Vec<CreateRoomValidationError>> {
    let mut errors = Vec::new();
    let room = match validate_voting_room(&input.question, &input.choices) {
        Ok(room) => Some(room),
        Err(field_errors) => {
            errors.push(CreateRoomValidationError::Fields(field_errors));
            None
        }
    };
    let visibility = input.visibility.as_deref().and_then(Visibility::parse);
    if visibility.is_none() {
        errors.push(CreateRoomValidationError::Visibility);
    }
    if errors.is_empty() {
        Ok((
            room.expect("validated room"),
            visibility.expect("validated visibility"),
        ))
    } else {
        Err(errors)
    }
}

pub async fn create_room(
    pool: &SqlitePool,
    input: &CreateRoomInput,
) -> Result<CreatedRoom, CreateRoomError> {
    create_room_with_token_source(pool, input, random_token).await
}

async fn create_room_with_token_source(
    pool: &SqlitePool,
    input: &CreateRoomInput,
    mut next_token: impl FnMut() -> Result<String, CreateRoomError>,
) -> Result<CreatedRoom, CreateRoomError> {
    let (room, visibility) = validate_create_room(input).map_err(CreateRoomError::Validation)?;
    let creator_token = next_token()?;
    let creator_hash = hash_token(&creator_token);

    for _ in 0..MAX_SLUG_ATTEMPTS {
        let slug = next_token()?;
        let mut transaction = pool.begin().await?;
        let inserted = sqlx::query(
            "INSERT INTO voting_rooms (
                slug, question, participant_names_public, creator_token_hash, created_at
             ) VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP)",
        )
        .bind(&slug)
        .bind(&room.question)
        .bind(visibility.names_public())
        .bind(&creator_hash)
        .execute(&mut *transaction)
        .await;

        let room_id = match inserted {
            Ok(result) => result.last_insert_rowid(),
            Err(error) if is_unique_slug_error(&error) => {
                transaction.rollback().await?;
                continue;
            }
            Err(error) => return Err(error.into()),
        };

        for (position, text) in room.choices.iter().enumerate() {
            sqlx::query(
                "INSERT INTO choices (room_id, id, text, position)
                 VALUES (?, ?, ?, ?)",
            )
            .bind(room_id)
            .bind(position as i64 + 1)
            .bind(text)
            .bind(position as i64)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        return Ok(CreatedRoom {
            id: room_id,
            slug,
            creator_token,
        });
    }
    Err(CreateRoomError::SlugCollision)
}

fn is_unique_slug_error(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|error| error.code())
        .is_some_and(|code| code == "2067")
        && error.to_string().contains("voting_rooms.slug")
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod integration_tests;
