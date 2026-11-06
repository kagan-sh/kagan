use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The agent programs kagan knows by name, with the label people know them by. The picker offers
/// one that is on the PATH under this exact command name.
pub const KNOWN_PROGRAMS: &[(&str, &str)] = &[
    ("claude", "Claude Code"),
    ("codex", "Codex"),
    ("gemini", "Gemini CLI"),
    ("opencode", "OpenCode"),
    ("cursor-agent", "Cursor Agent"),
    ("pi", "Pi"),
    ("amp", "Amp"),
    ("aider", "Aider"),
    ("goose", "Goose"),
    ("qwen", "Qwen Code"),
    ("copilot", "GitHub Copilot CLI"),
    ("grok", "Grok"),
    ("droid", "Droid"),
    ("kimi", "Kimi"),
    ("cline", "Cline"),
];

/// The known programs the picker lists even when they are not installed.
pub const LISTED_WHEN_MISSING: &[&str] = &["claude", "codex"];

/// Where a program was found in the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProgramSource {
    Known,
    Shell,
    Recent,
}

/// A program a new thread can run. `command` is the text `--program` takes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Program {
    pub command: String,
    pub label: String,
    pub source: ProgramSource,
    pub path: Option<PathBuf>,
    pub version: Option<String>,
}

impl Program {
    #[must_use]
    pub fn installed(&self) -> bool {
        self.path.is_some()
    }
}

/// The programs in the order the window shows them, and the one a new thread starts with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgramList {
    pub programs: Vec<Program>,
    pub default: Option<String>,
}

impl ProgramList {
    /// The first installed known agent, in the order the picker lists them. A shell or a recent
    /// command is never an agent.
    #[must_use]
    pub fn agent(&self) -> Option<&Program> {
        self.programs
            .iter()
            .find(|program| program.source == ProgramSource::Known && program.installed())
    }
}

/// Why a program line cannot be split into a command and its arguments.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SplitProgramError {
    #[error("program has an unfinished quote")]
    UnfinishedQuote,
    #[error("program is empty")]
    Empty,
}

/// Keeps the message for callers that collect program errors as strings.
impl From<SplitProgramError> for String {
    fn from(error: SplitProgramError) -> Self {
        error.to_string()
    }
}

/// The program named by `line`: words split on spaces, where single or double quotes keep spaces
/// together. A backslash is an ordinary character, so Windows paths work unquoted.
///
/// # Errors
///
/// The line has an unfinished quote, or no program name.
pub fn split_program(line: &str) -> Result<(String, Vec<String>), SplitProgramError> {
    let mut words = Vec::new();
    let mut word: Option<String> = None;
    let mut quote = None;
    for ch in line.chars() {
        match quote {
            Some(closing) if ch == closing => quote = None,
            None if ch == '\'' || ch == '"' => {
                quote = Some(ch);
                word.get_or_insert_default();
            }
            None if ch.is_whitespace() => words.extend(word.take()),
            _ => word.get_or_insert_default().push(ch),
        }
    }
    if quote.is_some() {
        return Err(SplitProgramError::UnfinishedQuote);
    }
    words.extend(word);
    let mut words = words.into_iter();
    let name = words.next().ok_or(SplitProgramError::Empty)?;
    if name.is_empty() {
        return Err(SplitProgramError::Empty);
    }
    Ok((name, words.collect()))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::{SplitProgramError, split_program};

    #[test]
    fn a_program_line_splits_on_spaces_and_keeps_quoted_words_together() {
        assert_eq!(
            split_program("claude --model 'fast model' \"two words\""),
            Ok((
                "claude".into(),
                vec!["--model".into(), "fast model".into(), "two words".into()]
            ))
        );
    }

    #[test]
    fn a_program_line_refuses_an_unfinished_quote_or_an_empty_program() {
        assert_eq!(
            split_program("'claude"),
            Err(SplitProgramError::UnfinishedQuote)
        );
        assert_eq!(split_program("  "), Err(SplitProgramError::Empty));
        assert_eq!(split_program("\"\""), Err(SplitProgramError::Empty));
    }

    proptest! {
        #[test]
        fn quoted_words_round_trip(words in prop::collection::vec("[^\"\\\\]{1,24}", 1..6)) {
            let line = words
                .iter()
                .map(|word| format!("\"{word}\""))
                .collect::<Vec<_>>()
                .join(" ");
            let (name, args) = split_program(&line).expect("quoted words");
            prop_assert_eq!(name, words[0].clone());
            prop_assert_eq!(args, words[1..].to_vec());
        }

        #[test]
        fn plain_words_round_trip(words in prop::collection::vec("[A-Za-z0-9._/:-]{1,24}", 1..6)) {
            let line = words.join(" ");
            let (name, args) = split_program(&line).expect("plain words");
            prop_assert_eq!(name, words[0].clone());
            prop_assert_eq!(args, words[1..].to_vec());
        }
    }

    #[test]
    fn the_refusal_keeps_the_message_callers_print() {
        assert_eq!(
            String::from(SplitProgramError::UnfinishedQuote),
            "program has an unfinished quote"
        );
        assert_eq!(String::from(SplitProgramError::Empty), "program is empty");
    }
}
