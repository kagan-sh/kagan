use kg_domain::{UsageProvider, UsageStatus};

use super::{answer, badge};

/// A made-up CLI transcript with the reply shapes the CLI gave on 2.1.322. The values are not
/// anyone's usage.
const TRANSCRIPT: &str = r#"{"type":"system","subtype":"init"}
{"type":"control_response","response":{"subtype":"success","request_id":"initialize","response":{"account":{"email":"person@example.test","subscriptionType":"max","apiProvider":"firstParty"},"commands":[]}}}
{"type":"control_response","response":{"subtype":"success","request_id":"usage","response":{"subscription_type":"max","rate_limits_available":true,"session":{"total_cost_usd":0},"rate_limits":{"five_hour":{"utilization":37,"resets_at":"2099-01-01T05:00:00.000000+00:00","limit_dollars":null},"seven_day":{"utilization":12,"resets_at":"2099-01-05T00:00:00.000000+00:00"},"seven_day_sonnet":null,"model_scoped":[{"display_name":"Sonnet","utilization":9,"resets_at":"2099-01-05T00:00:00.000000+00:00"}]}}}}
"#;

fn bars(status: &UsageStatus, windows: &[kg_domain::UsageWindow]) -> Vec<(String, f64)> {
    assert_eq!(*status, UsageStatus::Ok);
    windows
        .iter()
        .map(|window| (window.label.clone(), window.percent))
        .collect()
}

#[test]
fn the_usage_reply_becomes_five_hour_week_and_per_model_bars() {
    let account = answer(TRANSCRIPT.as_bytes(), 1).account;
    assert_eq!(account.provider, UsageProvider::Claude);
    assert_eq!(account.status, UsageStatus::Ok);
    assert_eq!(
        bars(&account.status, &account.windows),
        [
            ("5h".to_owned(), 37.0),
            ("7d".to_owned(), 12.0),
            ("7d · Sonnet".to_owned(), 9.0),
        ]
    );
    assert!(
        account
            .windows
            .iter()
            .all(|window| window.resets_at.is_some())
    );
    assert_eq!(account.plan.as_deref(), Some("Max"));
    assert_eq!(account.email.as_deref(), Some("person@example.test"));
}

#[test]
fn a_null_rate_limit_block_is_no_limits_not_zero() {
    let transcript = r#"{"type":"control_response","response":{"subtype":"success","request_id":"usage","response":{"subscription_type":null,"rate_limits_available":false,"rate_limits":null}}}"#;
    let account = answer(transcript.as_bytes(), 1).account;
    assert_eq!(account.status, UsageStatus::NoLimits);
    assert!(account.windows.is_empty());
}

#[test]
fn a_usage_reply_with_no_rate_limits_key_is_an_unreadable_reply() {
    let transcript = r#"{"type":"control_response","response":{"subtype":"success","request_id":"usage","response":{"session":{}}}}"#;
    let account = answer(transcript.as_bytes(), 1).account;
    assert_eq!(
        account.status,
        UsageStatus::Error {
            message: "Claude sent a reply kagan could not read.".to_owned()
        }
    );
}

#[test]
fn a_failed_usage_request_says_to_check_the_sign_in() {
    let transcript = r#"{"type":"control_response","response":{"subtype":"error","request_id":"usage","error":"not supported"}}"#;
    let account = answer(transcript.as_bytes(), 1).account;
    assert_eq!(
        account.status,
        UsageStatus::Error {
            message: "Claude could not give its usage. Check that Claude Code is signed in."
                .to_owned()
        }
    );
}

#[test]
fn output_that_is_not_json_is_an_unreadable_reply() {
    let account = answer(b"<html>\nnot json\n", 1).account;
    assert!(matches!(account.status, UsageStatus::Error { .. }));
    assert!(account.windows.is_empty());
}

#[test]
fn the_plan_badge_reads_the_subscription_name() {
    assert_eq!(badge("max").as_deref(), Some("Max"));
    assert_eq!(badge("claude_team").as_deref(), Some("Team"));
    assert_eq!(badge(""), None);
}
