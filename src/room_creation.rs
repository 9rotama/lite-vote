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
mod tests {
    use super::*;
    use crate::db::{DatabaseConfig, MIGRATOR, connect_pool};
    use std::time::Duration;
    use tempfile::tempdir;

    #[test]
    fn random_tokens_are_url_safe_and_have_256_bits() {
        let first = random_token().unwrap();
        let second = random_token().unwrap();
        assert_eq!(first.len(), ENCODED_TOKEN_LENGTH);
        assert_eq!(URL_SAFE_NO_PAD.decode(&first).unwrap().len(), TOKEN_BYTES);
        assert_ne!(first, second);
        assert!(
            first
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        );
    }

    #[test]
    fn token_hash_is_stable_and_does_not_reveal_the_token() {
        assert_eq!(hash_token("secret"), hash_token("secret"));
        assert_ne!(hash_token("secret"), hash_token("different"));
        assert!(!hash_token("secret").contains("secret"));
    }

    #[test]
    fn visibility_accepts_only_the_two_protocol_values() {
        assert_eq!(Visibility::parse("public"), Some(Visibility::Public));
        assert_eq!(Visibility::parse("anonymous"), Some(Visibility::Anonymous));
        for invalid in ["", "PUBLIC", "private", " public"] {
            assert_eq!(Visibility::parse(invalid), None);
        }
    }

    #[tokio::test]
    async fn retries_a_slug_collision_without_leaving_a_partial_room() {
        let dir = tempdir().unwrap();
        let pool = connect_pool(&DatabaseConfig {
            path: dir.path().join("collision.sqlite3"),
            busy_timeout: Duration::from_secs(5),
        })
        .await
        .unwrap();
        MIGRATOR.run(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO voting_rooms (
                slug, question, participant_names_public, creator_token_hash, created_at
             ) VALUES ('occupied', 'existing', 0, 'existing-hash', CURRENT_TIMESTAMP)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let mut tokens = ["creator-token", "occupied", "available"].into_iter();
        let created = create_room_with_token_source(
            &pool,
            &CreateRoomInput {
                question: "new room".into(),
                choices: vec!["first".into(), "second".into()],
                visibility: Some("public".into()),
            },
            || Ok(tokens.next().unwrap().to_owned()),
        )
        .await
        .unwrap();

        assert_eq!(created.slug, "available");
        assert_eq!(created.creator_token, "creator-token");
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM voting_rooms")
                .fetch_one(&pool)
                .await
                .unwrap(),
            2
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM choices
                 WHERE room_id = (SELECT id FROM voting_rooms WHERE slug = 'available')",
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            2
        );
    }
}
