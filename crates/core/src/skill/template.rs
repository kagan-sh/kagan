use kg_domain::SkillTemplate;

pub(super) fn text(name: &str, template: SkillTemplate) -> String {
    let (description, body) = match template {
        SkillTemplate::Blank => (
            "Describe when to use this skill.",
            "Write the instructions here.",
        ),
        SkillTemplate::ReviewPr => (
            "Review a pull request for bugs, risk and missing tests. Use when the person asks for a review.",
            "Read the pull request with `gh pr view` and `gh pr diff`. Check each change for bugs, unsafe edits and missing tests.\n\nReport findings first, most severe first. Each one names the file and line, and what breaks. End with a clear verdict: approve, or request changes.",
        ),
        SkillTemplate::ReleaseNotes => (
            "Write release notes from the commits since the last tag. Use when the person asks for release notes or a changelog.",
            "List the commits since the last tag with `git log`. Group them under Added, Changed, Fixed and Removed.\n\nWrite each line in plain words, one line per change, naming the user-visible effect. Leave out internal refactors and chores.",
        ),
        SkillTemplate::DebugTest => (
            "Find why a failing test fails and fix the cause. Use when a test or check fails.",
            "Run the failing test alone and read the full output. Find the line that first differs from what the test expects.\n\nTrace that value back to its source before you change anything. Fix the cause, then run the test again and the full suite.",
        ),
    };
    format!("---\nname: {name}\ndescription: {description}\n---\n\n{body}\n")
}

#[cfg(test)]
mod tests {
    use kg_domain::{Skill, SkillSource, SkillTemplate};

    use super::text;

    #[test]
    fn every_template_is_a_valid_skill_with_its_name() {
        for template in [
            SkillTemplate::Blank,
            SkillTemplate::ReviewPr,
            SkillTemplate::ReleaseNotes,
            SkillTemplate::DebugTest,
        ] {
            let body = text("demo-skill", template);
            let (skill, _) = Skill::parse("demo-skill", &body, SkillSource::User, None)
                .unwrap_or_else(|error| panic!("{template:?}: {error}"));
            assert_eq!(skill.name, "demo-skill");
        }
    }
}
