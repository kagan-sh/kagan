//! `kg usage` text: one block per signed-in provider, with its windows and reset times.

use kg_domain::{UsageAccount, UsageStatus, UsageView, resets_in};

pub(crate) fn format(view: &UsageView, now: i64) -> String {
    if !view.enabled {
        return "Provider usage is off. Turn it on with: kg preference set usage-limits true"
            .to_owned();
    }
    if view.accounts.is_empty() {
        return "No signed-in providers found.".to_owned();
    }
    view.accounts
        .iter()
        .map(|account| format_account(account, now))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn format_account(account: &UsageAccount, now: i64) -> String {
    let mut heading = account.provider.label().to_owned();
    if let Some(email) = &account.email {
        heading = format!("{heading} {email}");
    }
    if let Some(plan) = &account.plan {
        heading = format!("{heading} ({plan})");
    }
    let mut lines = vec![heading];
    match &account.status {
        UsageStatus::Ok | UsageStatus::Stale => {
            if account.status == UsageStatus::Stale {
                lines.push("Couldn't refresh. Showing last update.".to_owned());
            }
            for window in &account.windows {
                let reset = window.resets_at.map_or(String::new(), |at| {
                    format!(", resets in {}", resets_in(at, now))
                });
                lines.push(format!("  {}  {:.0}%{reset}", window.label, window.percent));
            }
        }
        UsageStatus::Expired => lines.push(format!(
            "Expired. Sign in again in {}.",
            account.provider.label().to_lowercase()
        )),
        UsageStatus::NoLimits => lines.push("No subscription limits for this sign-in.".to_owned()),
        UsageStatus::Error { message } => lines.push(message.clone()),
    }
    lines.join("\n")
}
