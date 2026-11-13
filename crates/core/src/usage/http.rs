//! The one HTTPS request a usage read makes. The token goes only to the host its provider pins,
//! no redirect is followed, and a reply is cut off at one mebibyte.

use std::io::Read as _;
use std::time::Duration;

use kg_domain::UsageStatus;

use super::error;

const LIMIT: u64 = 1024 * 1024;

pub(super) enum Refused {
    Status(UsageStatus),
    Busy(UsageStatus),
}

pub(super) fn allowed(url: &str, host: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(url) else {
        return false;
    };
    match url.scheme() {
        "https" => url.host_str() == Some(host),
        "http" => {
            cfg!(any(test, debug_assertions))
                && matches!(url.host_str(), Some("127.0.0.1" | "localhost"))
        }
        _ => false,
    }
}

pub(super) fn get(
    provider: &str,
    url: &str,
    host: &str,
    token: &str,
    headers: &[(&str, &str)],
    timeout: Duration,
) -> Result<Vec<u8>, Refused> {
    if !allowed(url, host) {
        return Err(Refused::Status(error(
            "Usage is only sent to the provider.",
        )));
    }
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(timeout)
        .user_agent(concat!("kagan/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| Refused::Status(error("Could not start the usage request.")))?;
    let mut request = client.get(url).bearer_auth(token);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = request
        .send()
        .map_err(|_| Refused::Status(error(&format!("Could not reach {provider}."))))?;
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(Refused::Status(UsageStatus::Expired));
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
        return Err(Refused::Busy(error(&format!(
            "{provider} answered {}.",
            status.as_u16()
        ))));
    }
    if !status.is_success() {
        return Err(Refused::Status(error(&format!(
            "{provider} answered {}.",
            status.as_u16()
        ))));
    }
    let mut body = Vec::new();
    response
        .take(LIMIT)
        .read_to_end(&mut body)
        .map_err(|_| Refused::Status(error(&format!("{provider}'s reply stopped early."))))?;
    Ok(body)
}
