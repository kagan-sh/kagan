pub(crate) fn child_notice(title: &str, error: Option<&str>, before_turn: bool) -> String {
    let because = error.map(|error| format!(": {error}")).unwrap_or_default();
    if before_turn {
        format!("Child thread \"{title}\" failed before its first turn{because}.")
    } else {
        format!("Child thread \"{title}\" failed{because}.")
    }
}
