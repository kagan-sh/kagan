use std::path::PathBuf;

use clap::{Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

use crate::ProjectId;

use super::OpError;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum SkillOp {
    /// Show the skills page, opening the window if it is closed
    Open,
    /// List skills
    List {
        /// Project whose skills to include
        #[arg(long, env = "KG_PROJECT_ID")]
        project_id: Option<ProjectId>,
    },
    /// Show one skill
    Show {
        /// Skill name
        name: String,
        /// Project whose skills to include
        #[arg(long, env = "KG_PROJECT_ID")]
        project_id: Option<ProjectId>,
    },
    /// List a skill's files
    Files {
        /// Skill name
        name: String,
        /// Project whose skills to include
        #[arg(long, env = "KG_PROJECT_ID")]
        project_id: Option<ProjectId>,
    },
    /// Create a skill
    New {
        /// Skill name, lowercase and hyphenated
        name: String,
        /// Create it in the project folder instead of the user folder
        #[arg(long)]
        project: bool,
        /// What the new SKILL.md starts with
        #[arg(long, value_enum, default_value_t = SkillTemplate::Blank)]
        #[serde(default)]
        template: SkillTemplate,
        /// Project whose skills to include
        #[arg(long, env = "KG_PROJECT_ID")]
        project_id: Option<ProjectId>,
    },
    /// Add a skill from a folder
    Add {
        /// Folder to read the skill from
        path: PathBuf,
        /// Add it to the project folder instead of the user folder
        #[arg(long)]
        project: bool,
        /// Project whose skills to include
        #[arg(long, env = "KG_PROJECT_ID")]
        project_id: Option<ProjectId>,
    },
    /// Replace a user or project skill's SKILL.md text
    Update {
        /// Skill name
        name: String,
        /// The whole SKILL.md, frontmatter included
        text: String,
        /// The SKILL.md text the window read; refused when the file has changed since
        #[arg(skip)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected: Option<String>,
        /// Project whose skills to include
        #[arg(long, env = "KG_PROJECT_ID")]
        project_id: Option<ProjectId>,
    },
    /// Remove a skill
    Remove {
        /// Skill name
        name: String,
        /// Project whose skills to include
        #[arg(long, env = "KG_PROJECT_ID")]
        project_id: Option<ProjectId>,
    },
}

/// The starting text of a new skill.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum SkillTemplate {
    /// A placeholder to fill in
    #[default]
    Blank,
    /// Review a pull request
    ReviewPr,
    /// Write release notes
    ReleaseNotes,
    /// Debug a failing test
    DebugTest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Skill {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub hint: Option<String>,
    pub source: SkillSource,
    pub path: Option<PathBuf>,
}

impl Skill {
    /// # Errors
    /// When frontmatter is missing, invalid, or names a different folder.
    pub fn parse(
        name: &str,
        text: &str,
        source: SkillSource,
        path: Option<PathBuf>,
    ) -> Result<(Self, String), OpError> {
        #[derive(Deserialize)]
        struct Header {
            name: String,
            description: String,
            #[serde(default, rename = "argument-hint")]
            argument_hint: Option<String>,
        }
        let text = text.replace("\r\n", "\n");
        let (yaml, body) = text
            .strip_prefix("---\n")
            .and_then(|text| text.split_once("\n---\n"))
            .ok_or_else(|| OpError::Invalid("SKILL.md needs YAML frontmatter".into()))?;
        let header: Header =
            serde_yaml_ng::from_str(yaml).map_err(|error| OpError::Invalid(error.to_string()))?;
        let valid = !name.is_empty()
            && name.split('-').all(|part| {
                !part.is_empty()
                    && part
                        .bytes()
                        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
            });
        if header.name != name
            || !valid
            || header.description.trim().is_empty()
            || header.description.chars().any(crate::unsafe_char)
        {
            return Err(OpError::Invalid("skill name must match its lowercase hyphenated folder; description must be one nonempty line".into()));
        }
        let hint = header
            .argument_hint
            .map(|hint| hint.trim().to_owned())
            .filter(|hint| !hint.is_empty());
        if hint
            .as_ref()
            .is_some_and(|hint| hint.chars().any(crate::unsafe_char))
        {
            return Err(OpError::Invalid("argument-hint must be one line".into()));
        }
        Ok((
            Self {
                name: header.name,
                description: header.description,
                hint,
                source,
                path,
            },
            body.trim().to_owned(),
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillSource {
    Bundled,
    Plugin,
    User,
    Project,
}

#[cfg(test)]
mod tests {
    use super::{Skill, SkillSource};

    const SKILL: &str = "\
---
name: demo
description: Does one thing
argument-hint: <path>
---

Body
";

    #[test]
    fn a_skill_reads_its_frontmatter_and_body() {
        let (skill, body) = Skill::parse("demo", SKILL, SkillSource::User, None).expect("a skill");
        assert_eq!(skill.name, "demo");
        assert_eq!(skill.description, "Does one thing");
        assert_eq!(skill.hint.as_deref(), Some("<path>"));
        assert_eq!(body, "Body");
    }

    #[test]
    fn a_skill_refuses_a_mismatched_name_or_missing_frontmatter() {
        assert!(Skill::parse("other", SKILL, SkillSource::User, None).is_err());
        assert!(Skill::parse("demo", "Body", SkillSource::User, None).is_err());
        let bad_hint = SKILL.replace("<path>", "\"two\\nlines\"");
        assert!(Skill::parse("demo", &bad_hint, SkillSource::User, None).is_err());
    }
}
