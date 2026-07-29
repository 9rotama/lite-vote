use super::*;

#[test]
fn input_boundaries_are_counted_in_graphemes() {
    let cases = [
        (
            ValidationField::Question,
            200,
            validate_question as fn(&str) -> Result<String, ValidationError>,
        ),
        (
            ValidationField::DisplayName,
            30,
            validate_display_name as fn(&str) -> Result<String, ValidationError>,
        ),
    ];

    for (field, max, validate) in cases {
        assert_eq!(
            validate(""),
            Err(ValidationError {
                field,
                reason: ValidationErrorReason::Empty,
            })
        );
        assert!(validate("a").is_ok());
        assert!(validate(&"a".repeat(max)).is_ok());
        assert_eq!(
            validate(&"a".repeat(max + 1)),
            Err(ValidationError {
                field,
                reason: ValidationErrorReason::TooLong {
                    max,
                    actual: max + 1,
                },
            })
        );
    }

    assert!(validate_choice("a", 4).is_ok());
    assert!(validate_choice(&"a".repeat(100), 4).is_ok());
    assert_eq!(
        validate_choice("", 4),
        Err(ValidationError {
            field: ValidationField::Choice(4),
            reason: ValidationErrorReason::Empty,
        })
    );
    assert_eq!(
        validate_choice(&"a".repeat(101), 4),
        Err(ValidationError {
            field: ValidationField::Choice(4),
            reason: ValidationErrorReason::TooLong {
                max: 100,
                actual: 101,
            },
        })
    );
}

#[test]
fn combined_unicode_sequences_count_as_one_grapheme() {
    assert_eq!(validate_question("e\u{301}").unwrap(), "e\u{301}");
    assert_eq!(validate_question("👨‍👩‍👧‍👦").unwrap(), "👨‍👩‍👧‍👦");

    let combining = "e\u{301}".repeat(200);
    let emoji = "👩🏽‍💻".repeat(200);
    assert!(validate_question(&combining).is_ok());
    assert!(validate_question(&emoji).is_ok());
    assert!(validate_question(&(combining + "x")).is_err());
    assert!(validate_question(&(emoji + "x")).is_err());
}

#[test]
fn unicode_whitespace_is_trimmed_and_interior_controls_are_rejected() {
    assert_eq!(
        validate_question("\t\n\u{3000} question \u{00a0}\r\n").unwrap(),
        "question"
    );
    assert_eq!(
        validate_question("question\ncontinued"),
        Err(ValidationError {
            field: ValidationField::Question,
            reason: ValidationErrorReason::ContainsControlCharacter,
        })
    );
}

#[test]
fn choice_count_boundaries_are_validated() {
    for accepted in [2, 10] {
        let choices: Vec<_> = (0..accepted)
            .map(|index| format!("choice {index}"))
            .collect();
        assert!(validate_voting_room("question", &choices).is_ok());
    }
    for rejected in [1, 11] {
        let choices: Vec<_> = (0..rejected)
            .map(|index| format!("choice {index}"))
            .collect();
        let errors = validate_voting_room("question", &choices).unwrap_err();
        assert!(errors.contains(&ValidationError {
            field: ValidationField::ChoiceList,
            reason: ValidationErrorReason::InvalidChoiceCount {
                min: 2,
                max: 10,
                actual: rejected,
            },
        }));
    }
}

#[test]
fn duplicates_use_exact_trimmed_values_only() {
    let errors = validate_voting_room("question", &[" same ", "same"]).unwrap_err();
    assert_eq!(
        errors,
        vec![ValidationError {
            field: ValidationField::Choice(1),
            reason: ValidationErrorReason::DuplicateChoice { first_index: 0 },
        }]
    );

    assert!(validate_voting_room("question", &["Choice", "choice"]).is_ok());
    assert!(validate_voting_room("question", &["é", "e\u{301}"]).is_ok());
}

#[test]
fn all_invalid_fields_and_choice_positions_are_returned_together() {
    let errors =
        validate_voting_room("", &["valid", " ", "bad\u{0000}value", "valid"]).unwrap_err();
    assert_eq!(
        errors,
        vec![
            ValidationError {
                field: ValidationField::Question,
                reason: ValidationErrorReason::Empty,
            },
            ValidationError {
                field: ValidationField::Choice(1),
                reason: ValidationErrorReason::Empty,
            },
            ValidationError {
                field: ValidationField::Choice(2),
                reason: ValidationErrorReason::ContainsControlCharacter,
            },
            ValidationError {
                field: ValidationField::Choice(3),
                reason: ValidationErrorReason::DuplicateChoice { first_index: 0 },
            },
        ]
    );
}

#[test]
fn valid_room_returns_only_trimmed_values() {
    assert_eq!(
        validate_voting_room(" question ", &[" first\n", "\tsecond "]).unwrap(),
        ValidatedVotingRoom {
            question: "question".to_owned(),
            choices: vec!["first".to_owned(), "second".to_owned()],
        }
    );
}
