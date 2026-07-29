//! Vote result calculation.

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

#[cfg(test)]
mod tests;
