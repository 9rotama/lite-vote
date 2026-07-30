//! Vote result calculation and persistence-backed result loading.

use sqlx::{FromRow, SqlitePool};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChoiceVotes {
    pub choice_id: i64,
    pub vote_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChoiceResult {
    pub choice_id: i64,
    pub vote_count: u64,
    pub percentage_tenths: u16,
    pub is_winner: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayChoiceResult {
    pub text: String,
    pub vote_count: u64,
    pub percentage_tenths: u16,
    pub is_winner: bool,
    pub voter_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomResults {
    pub choices: Vec<DisplayChoiceResult>,
    pub total_votes: u64,
}

#[derive(FromRow)]
struct StoredChoiceVotes {
    choice_id: i64,
    text: String,
    vote_count: i64,
}

#[derive(FromRow)]
struct StoredVoter {
    choice_id: i64,
    display_name: String,
}

pub fn calculate_results(choices: &[ChoiceVotes], is_closed: bool) -> Vec<ChoiceResult> {
    let total_votes: u128 = choices
        .iter()
        .map(|choice| u128::from(choice.vote_count))
        .sum();
    let winning_vote_count = if is_closed && total_votes > 0 {
        choices.iter().map(|choice| choice.vote_count).max()
    } else {
        None
    };

    choices
        .iter()
        .map(|choice| {
            let percentage_tenths = match total_votes {
                0 => 0,
                total_votes => {
                    let rounded =
                        (u128::from(choice.vote_count) * 1_000 + total_votes / 2) / total_votes;
                    u16::try_from(rounded).expect("a percentage in tenths cannot exceed 1000")
                }
            };
            ChoiceResult {
                choice_id: choice.choice_id,
                vote_count: choice.vote_count,
                percentage_tenths,
                is_winner: winning_vote_count == Some(choice.vote_count),
            }
        })
        .collect()
}

pub async fn load_results(
    pool: &SqlitePool,
    room_id: i64,
    is_closed: bool,
    participant_names_public: bool,
) -> Result<RoomResults, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let stored: Vec<StoredChoiceVotes> = sqlx::query_as(
        "SELECT choices.id AS choice_id, choices.text,
                COUNT(votes.participant_id) AS vote_count
         FROM choices
         LEFT JOIN votes
           ON votes.room_id = choices.room_id
          AND votes.choice_id = choices.id
         WHERE choices.room_id = ?
         GROUP BY choices.id, choices.text, choices.position
         ORDER BY choices.position",
    )
    .bind(room_id)
    .fetch_all(&mut *transaction)
    .await?;
    let counts = stored
        .iter()
        .map(|choice| ChoiceVotes {
            choice_id: choice.choice_id,
            vote_count: u64::try_from(choice.vote_count)
                .expect("SQLite COUNT cannot return a negative value"),
        })
        .collect::<Vec<_>>();
    let calculated = calculate_results(&counts, is_closed);
    let voters: Vec<StoredVoter> = if participant_names_public {
        sqlx::query_as(
            "SELECT votes.choice_id, participants.display_name
             FROM votes
             JOIN participants
               ON participants.room_id = votes.room_id
              AND participants.id = votes.participant_id
             JOIN choices
               ON choices.room_id = votes.room_id
              AND choices.id = votes.choice_id
             WHERE votes.room_id = ?
               AND participants.display_name IS NOT NULL
             ORDER BY choices.position, participants.id",
        )
        .bind(room_id)
        .fetch_all(&mut *transaction)
        .await?
    } else {
        Vec::new()
    };
    let choices = stored
        .into_iter()
        .zip(calculated)
        .map(|(stored, calculated)| DisplayChoiceResult {
            text: stored.text,
            vote_count: calculated.vote_count,
            percentage_tenths: calculated.percentage_tenths,
            is_winner: calculated.is_winner,
            voter_names: voters
                .iter()
                .filter(|voter| voter.choice_id == calculated.choice_id)
                .map(|voter| voter.display_name.clone())
                .collect(),
        })
        .collect();
    let results = RoomResults {
        choices,
        total_votes: counts.iter().map(|choice| choice.vote_count).sum(),
    };
    transaction.commit().await?;
    Ok(results)
}

#[cfg(test)]
mod tests;
