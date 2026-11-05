use std::collections::BTreeSet;

use gpui_shell::HttpRequestGrant;
use serde::{Deserialize, Serialize};

/// The most loopback fetch entries a manifest may declare.
pub(crate) const LOCAL_FETCH_CAP: usize = 8;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Powers {
    pub(crate) run: Vec<Program>,
    pub(crate) fetch: Vec<Fetch>,
    pub(crate) clipboard: Clipboard,
    pub(crate) areas: BTreeSet<Area>,
    pub(crate) read: Vec<String>,
    #[serde(default, rename = "openUrl")]
    pub(crate) open_url: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Program {
    pub(crate) program: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) env: Option<Env>,
}

impl Program {
    pub(crate) fn within(&self, approved: &Self) -> bool {
        self.program == approved.program
            && (self.env.is_none() || approved.env == Some(Env::Inherit))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Env {
    Inherit,
}

/// Why a fetch host cannot be granted.
#[derive(Debug, thiserror::Error)]
pub(crate) enum FetchError {
    #[error("fetch host {0:?} must be http://127.0.0.1:<port> or http://localhost:<port>")]
    BareLoopback(String),
    #[error("loopback fetch needs an approved explicit HTTP port (IPv6 is unsupported)")]
    LoopbackPort,
    #[error("fetch host {0:?} must be a hostname, or http://127.0.0.1:<port>")]
    NotHostname(String),
    #[error("fetch host {0:?} needs a host")]
    NoHost(String),
    #[error("http fetch is only for 127.0.0.1 or localhost, not {0:?}")]
    HttpNotLoopback(String),
    #[error("local fetch {0:?} must be only http://host:port")]
    LocalShape(String),
    #[error("local fetch {0:?} needs an explicit port")]
    NeedsPort(String),
    #[error("local fetch {0:?} needs a valid explicit port")]
    BadPort(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Fetch {
    pub(crate) host: String,
    pub(crate) methods: BTreeSet<Method>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FetchKind {
    Remote,
    Loopback { host: String, port: u16 },
}

impl Fetch {
    pub(crate) fn kind(&self) -> Result<FetchKind, FetchError> {
        parse_fetch_host(&self.host)
    }

    pub(crate) fn view_host(&self) -> Result<String, FetchError> {
        match self.kind()? {
            FetchKind::Remote => Ok(self.host.clone()),
            FetchKind::Loopback { host, .. } => Ok(host),
        }
    }

    pub(crate) fn http_grant(&self) -> Result<HttpRequestGrant, FetchError> {
        let methods = self.methods.iter().map(|method| method.name());
        Ok(match self.kind()? {
            FetchKind::Remote => {
                HttpRequestGrant::new(self.host.clone(), methods, Vec::<String>::new(), ["/"])
            }
            FetchKind::Loopback { host, port } => {
                HttpRequestGrant::new(host, methods, Vec::<String>::new(), ["/"])
                    .scheme("http")
                    .port(port)
            }
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) enum Method {
    #[serde(rename = "GET")]
    Get,
    #[serde(rename = "POST")]
    Post,
}

impl Method {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
        }
    }
}

fn parse_fetch_host(raw: &str) -> Result<FetchKind, FetchError> {
    if raw.eq_ignore_ascii_case("127.0.0.1") || raw.eq_ignore_ascii_case("localhost") {
        return Err(FetchError::BareLoopback(raw.to_owned()));
    }
    if reaches_this_computer(raw) {
        return Err(FetchError::LoopbackPort);
    }
    match url::Url::parse(raw) {
        Ok(url) => {
            if url.scheme() != "http" {
                return Err(FetchError::NotHostname(raw.to_owned()));
            }
            let Some(host) = url.host_str() else {
                return Err(FetchError::NoHost(raw.to_owned()));
            };
            if !host.eq_ignore_ascii_case("127.0.0.1") && !host.eq_ignore_ascii_case("localhost") {
                return Err(FetchError::HttpNotLoopback(host.to_owned()));
            }
            let port = explicit_loopback_port(raw)?;
            if url.path() != "/" && !url.path().is_empty()
                || url.query().is_some()
                || url.fragment().is_some()
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return Err(FetchError::LocalShape(raw.to_owned()));
            }
            Ok(FetchKind::Loopback {
                host: host.to_ascii_lowercase(),
                port,
            })
        }
        Err(_)
            if raw.contains("://")
                || raw.contains(':')
                || raw.is_empty()
                || raw.contains('/')
                || raw.contains(' ') =>
        {
            Err(FetchError::NotHostname(raw.to_owned()))
        }
        Err(_) => Ok(FetchKind::Remote),
    }
}

// Names that dial this computer must not become a remote HTTPS grant.
// Every IPv6 literal counts as this computer, not only IPv6 loopback.
fn reaches_this_computer(raw: &str) -> bool {
    let name = raw.trim_start_matches('[').trim_end_matches(']');
    let bare = name.trim_end_matches('.');
    let dns = bare.to_ascii_lowercase();
    if dns == "localhost" || dns.ends_with(".localhost") {
        return true;
    }
    match bare.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => ip.is_loopback() || ip.is_unspecified(),
        Ok(std::net::IpAddr::V6(_)) => true,
        Err(_) => false,
    }
}

// `Url::port` drops a scheme's default, so `http://127.0.0.1:80` looks unported.
fn explicit_loopback_port(raw: &str) -> Result<u16, FetchError> {
    let Some((_, authority)) = raw.split_once("://") else {
        return Err(FetchError::NeedsPort(raw.to_owned()));
    };
    let authority = authority
        .split_once(['/', '?', '#'])
        .map_or(authority, |(head, _)| head);
    let Some((_, port)) = authority.rsplit_once(':') else {
        return Err(FetchError::NeedsPort(raw.to_owned()));
    };
    port.parse::<u16>()
        .map_err(|_| FetchError::BadPort(raw.to_owned()))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Clipboard {
    pub(crate) read: bool,
    pub(crate) write: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Area {
    Threads,
    Database,
    Process,
}

impl Area {
    pub(crate) const ALL: [Self; 3] = [Self::Threads, Self::Database, Self::Process];

    pub(crate) fn id(self) -> &'static str {
        self.row().0
    }

    pub(crate) fn name(self) -> &'static str {
        self.row().1
    }

    pub(crate) fn phrase(self) -> &'static str {
        self.row().2
    }

    fn row(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Threads => (
                "threads",
                "Threads",
                "reads, starts and changes your threads, can type into threads and press Enter, reads their screen text, and reads their changed files",
            ),
            Self::Database => ("database", "Database", "keeps its own database"),
            Self::Process => (
                "process",
                "Process",
                "starts and stops the programs listed here",
            ),
        }
    }
}
