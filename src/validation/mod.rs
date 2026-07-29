//! Voting room input validation.

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
mod tests;
