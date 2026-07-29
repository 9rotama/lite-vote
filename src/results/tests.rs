use super::*;

fn choice(choice_id: i64, vote_count: u64) -> ChoiceVotes {
    ChoiceVotes {
        choice_id,
        vote_count,
    }
}

#[test]
fn zero_votes_have_zero_percent_and_no_winner_even_when_closed() {
    assert_eq!(
        calculate_results(&[choice(1, 0), choice(2, 0)], true),
        vec![
            ChoiceResult {
                choice_id: 1,
                vote_count: 0,
                percentage_tenths: 0,
                is_winner: false,
            },
            ChoiceResult {
                choice_id: 2,
                vote_count: 0,
                percentage_tenths: 0,
                is_winner: false,
            },
        ]
    );
}

#[test]
fn percentages_are_rounded_independently() {
    let results = calculate_results(&[choice(1, 1), choice(2, 2)], false);
    assert_eq!(results[0].percentage_tenths, 333);
    assert_eq!(results[1].percentage_tenths, 667);
}

#[test]
fn all_votes_are_one_hundred_percent() {
    let results = calculate_results(&[choice(1, 3), choice(2, 0)], false);
    assert_eq!(results[0].percentage_tenths, 1_000);
    assert_eq!(results[1].percentage_tenths, 0);
}

#[test]
fn open_vote_has_no_winner() {
    let results = calculate_results(&[choice(1, 3), choice(2, 1)], false);
    assert!(results.iter().all(|result| !result.is_winner));
}

#[test]
fn closed_vote_marks_the_single_winner() {
    let results = calculate_results(&[choice(1, 3), choice(2, 1)], true);
    assert!(results[0].is_winner);
    assert!(!results[1].is_winner);
}

#[test]
fn closed_vote_marks_all_tied_winners() {
    let results = calculate_results(&[choice(1, 3), choice(2, 3), choice(3, 1)], true);
    assert!(results[0].is_winner);
    assert!(results[1].is_winner);
    assert!(!results[2].is_winner);
}
