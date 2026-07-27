use std::collections::HashMap;

use unicode_segmentation::UnicodeSegmentation;

pub const QUESTION_MAX_GRAPHEMES: usize = 200;
pub const CHOICE_MAX_GRAPHEMES: usize = 100;
pub const DISPLAY_NAME_MAX_GRAPHEMES: usize = 30;
pub const MIN_CHOICES: usize = 2;
pub const MAX_CHOICES: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationField {
    Question,
    Choice(usize),
    ChoiceList,
    DisplayName,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationErrorReason {
    Empty,
    ContainsControlCharacter,
    TooLong {
        max: usize,
        actual: usize,
    },
    InvalidChoiceCount {
        min: usize,
        max: usize,
        actual: usize,
    },
    DuplicateChoice {
        first_index: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub field: ValidationField,
    pub reason: ValidationErrorReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedVotingRoom {
    pub question: String,
    pub choices: Vec<String>,
}

pub fn validate_question(input: &str) -> Result<String, ValidationError> {
    validate_text(input, ValidationField::Question, QUESTION_MAX_GRAPHEMES)
}

pub fn validate_choice(input: &str, index: usize) -> Result<String, ValidationError> {
    validate_text(input, ValidationField::Choice(index), CHOICE_MAX_GRAPHEMES)
}

pub fn validate_display_name(input: &str) -> Result<String, ValidationError> {
    validate_text(
        input,
        ValidationField::DisplayName,
        DISPLAY_NAME_MAX_GRAPHEMES,
    )
}

pub fn validate_voting_room(
    question: &str,
    choices: &[impl AsRef<str>],
) -> Result<ValidatedVotingRoom, Vec<ValidationError>> {
    let mut errors = Vec::new();
    let validated_question = match validate_question(question) {
        Ok(question) => Some(question),
        Err(error) => {
            errors.push(error);
            None
        }
    };

    if !(MIN_CHOICES..=MAX_CHOICES).contains(&choices.len()) {
        errors.push(ValidationError {
            field: ValidationField::ChoiceList,
            reason: ValidationErrorReason::InvalidChoiceCount {
                min: MIN_CHOICES,
                max: MAX_CHOICES,
                actual: choices.len(),
            },
        });
    }

    let mut validated_choices = Vec::with_capacity(choices.len());
    let mut first_index_by_value = HashMap::new();
    for (index, choice) in choices.iter().enumerate() {
        match validate_choice(choice.as_ref(), index) {
            Ok(choice) => {
                if let Some(first_index) = first_index_by_value.get(&choice) {
                    errors.push(ValidationError {
                        field: ValidationField::Choice(index),
                        reason: ValidationErrorReason::DuplicateChoice {
                            first_index: *first_index,
                        },
                    });
                } else {
                    first_index_by_value.insert(choice.clone(), index);
                }
                validated_choices.push(choice);
            }
            Err(error) => {
                errors.push(error);
            }
        }
    }

    if errors.is_empty() {
        Ok(ValidatedVotingRoom {
            question: validated_question.expect("a question is present when validation succeeds"),
            choices: validated_choices,
        })
    } else {
        Err(errors)
    }
}

fn validate_text(
    input: &str,
    field: ValidationField,
    max_graphemes: usize,
) -> Result<String, ValidationError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(ValidationError {
            field,
            reason: ValidationErrorReason::Empty,
        });
    }
    if trimmed.chars().any(char::is_control) {
        return Err(ValidationError {
            field,
            reason: ValidationErrorReason::ContainsControlCharacter,
        });
    }

    let actual = trimmed.graphemes(true).count();
    if actual > max_graphemes {
        return Err(ValidationError {
            field,
            reason: ValidationErrorReason::TooLong {
                max: max_graphemes,
                actual,
            },
        });
    }

    Ok(trimmed.to_owned())
}

#[cfg(test)]
mod tests {
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
}
