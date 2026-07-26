use sqlx::{FromRow, Sqlite, SqlitePool, Transaction};

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct VotingRoom {
    pub id: i64,
    pub slug: String,
    pub question: String,
    pub participant_names_public: bool,
    pub creator_token_hash: String,
    pub created_at: String,
    pub closed_at: Option<String>,
}

impl VotingRoom {
    pub fn is_closed(&self) -> bool {
        self.closed_at.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct Choice {
    pub room_id: i64,
    pub id: i64,
    pub text: String,
    pub position: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct Participant {
    pub room_id: i64,
    pub id: i64,
    pub token_hash: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct Vote {
    pub room_id: i64,
    pub participant_id: i64,
    pub choice_id: i64,
}

pub async fn insert_voting_room(
    transaction: &mut Transaction<'_, Sqlite>,
    room: &VotingRoom,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO voting_rooms (
            id, slug, question, participant_names_public, creator_token_hash, created_at, closed_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(room.id)
    .bind(&room.slug)
    .bind(&room.question)
    .bind(room.participant_names_public)
    .bind(&room.creator_token_hash)
    .bind(&room.created_at)
    .bind(&room.closed_at)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

pub async fn insert_choice(
    transaction: &mut Transaction<'_, Sqlite>,
    choice: &Choice,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO choices (room_id, id, text, position) VALUES (?, ?, ?, ?)")
        .bind(choice.room_id)
        .bind(choice.id)
        .bind(choice.text.trim())
        .bind(choice.position)
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

pub async fn insert_participant(
    transaction: &mut Transaction<'_, Sqlite>,
    participant: &Participant,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO participants (room_id, id, token_hash, display_name) VALUES (?, ?, ?, ?)",
    )
    .bind(participant.room_id)
    .bind(participant.id)
    .bind(&participant.token_hash)
    .bind(&participant.display_name)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

pub async fn insert_vote(
    transaction: &mut Transaction<'_, Sqlite>,
    vote: &Vote,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO votes (room_id, participant_id, choice_id) VALUES (?, ?, ?)")
        .bind(vote.room_id)
        .bind(vote.participant_id)
        .bind(vote.choice_id)
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

pub async fn update_vote_choice(
    transaction: &mut Transaction<'_, Sqlite>,
    room_id: i64,
    participant_id: i64,
    choice_id: i64,
) -> Result<bool, sqlx::Error> {
    let result =
        sqlx::query("UPDATE votes SET choice_id = ? WHERE room_id = ? AND participant_id = ?")
            .bind(choice_id)
            .bind(room_id)
            .bind(participant_id)
            .execute(&mut **transaction)
            .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn replace_choices(
    transaction: &mut Transaction<'_, Sqlite>,
    room_id: i64,
    choices: &[Choice],
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM choices WHERE room_id = ?")
        .bind(room_id)
        .execute(&mut **transaction)
        .await?;
    for choice in choices {
        insert_choice(transaction, choice).await?;
    }
    Ok(())
}

pub async fn find_voting_room(
    pool: &SqlitePool,
    id: i64,
) -> Result<Option<VotingRoom>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, slug, question, participant_names_public, creator_token_hash,
                created_at, closed_at
         FROM voting_rooms WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn find_choice(
    pool: &SqlitePool,
    room_id: i64,
    id: i64,
) -> Result<Option<Choice>, sqlx::Error> {
    sqlx::query_as("SELECT room_id, id, text, position FROM choices WHERE room_id = ? AND id = ?")
        .bind(room_id)
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn find_participant(
    pool: &SqlitePool,
    room_id: i64,
    id: i64,
) -> Result<Option<Participant>, sqlx::Error> {
    sqlx::query_as(
        "SELECT room_id, id, token_hash, display_name
         FROM participants WHERE room_id = ? AND id = ?",
    )
    .bind(room_id)
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn find_vote(
    pool: &SqlitePool,
    room_id: i64,
    participant_id: i64,
) -> Result<Option<Vote>, sqlx::Error> {
    sqlx::query_as(
        "SELECT room_id, participant_id, choice_id
         FROM votes WHERE room_id = ? AND participant_id = ?",
    )
    .bind(room_id)
    .bind(participant_id)
    .fetch_optional(pool)
    .await
}
