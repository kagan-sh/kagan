use super::{CliGroup, Error};
use crate::runtime::strip::forbidden_status_char;

const USAGE_CAP: usize = 200;

/// Group and command names become `kg <group> <command>`, and usage text lands in a skill, so both are checked here.
pub(super) fn check(groups: &[CliGroup]) -> Result<(), Error> {
    for (index, group) in groups.iter().enumerate() {
        if !plain_word(&group.name) {
            return Err(Error::Cli(format!(
                "cli[{index}].name {:?} must be lowercase letters, digits and dashes, at most 40 characters",
                group.name
            )));
        }
        for (position, command) in group.commands.iter().enumerate() {
            if !plain_word(&command.name) {
                return Err(Error::Cli(format!(
                    "cli[{index}].commands[{position}].name {:?} must be lowercase letters, digits and dashes, at most 40 characters",
                    command.name
                )));
            }
            if command
                .usage
                .as_deref()
                .is_some_and(|usage| !plain_usage(usage))
            {
                return Err(Error::Cli(format!(
                    "cli[{index}].commands[{position}].usage must be one plain line of at most {USAGE_CAP} characters, with no backtick, control or bidi characters"
                )));
            }
        }
    }
    Ok(())
}

fn plain_word(word: &str) -> bool {
    !word.is_empty()
        && word.len() <= 40
        && word
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn plain_usage(usage: &str) -> bool {
    !usage.trim().is_empty()
        && usage.chars().count() <= USAGE_CAP
        && !usage.contains('`')
        && !usage.chars().any(forbidden_status_char)
}
