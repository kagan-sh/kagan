use kg_domain::EventKind;

pub(crate) const QUIT: &str = "kagan quit while the program was running";
pub(crate) const STARTING: &str = "kagan stopped while the thread was starting";

pub(crate) enum End {
    Stopped,
    Exited(u32),
    Signal(i32),
    Quit,
    Starting,
    Failed(String),
}

impl End {
    // `kinds` is newest first.
    pub(crate) fn from_newest(kinds: &[EventKind]) -> Option<Self> {
        let kind = kinds.first()?;
        Some(match kind {
            EventKind::Stopped => Self::Stopped,
            EventKind::Exited {
                signal: Some(signal),
                ..
            } => Self::Signal(*signal),
            EventKind::Exited {
                code: Some(code),
                signal: None,
            } => Self::Exited(*code),
            EventKind::Failed { error } => {
                if let Some(earlier) = kinds.get(1)
                    && let Some(end) = Self::from_newest(std::slice::from_ref(earlier))
                    && matches!(end, Self::Exited(_) | Self::Signal(_))
                {
                    return Some(end);
                }
                Self::from_failed(error.as_deref())
            }
            _ => return None,
        })
    }

    fn from_failed(error: Option<&str>) -> Self {
        match error {
            Some(QUIT) => Self::Quit,
            Some(STARTING) => Self::Starting,
            Some(text) => Self::Failed(text.to_owned()),
            None => Self::Failed("The program failed.".into()),
        }
    }

    pub(crate) fn line(&self) -> String {
        match self {
            Self::Stopped => "Stopped.".into(),
            Self::Exited(code) => format!("Exited with code {code}."),
            Self::Signal(signal) => format!("Ended by signal {signal}."),
            Self::Quit => "Ended when kagan quit. No output was saved for that run. Use Start again to restart it.".into(),
            Self::Starting => "Stopped while starting.".into(),
            Self::Failed(text) => text.clone(),
        }
    }

    pub(crate) fn shows_mark(&self) -> bool {
        matches!(self, Self::Stopped | Self::Exited(_))
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Loss {
    History,
    Screen,
}

impl Loss {
    pub(crate) fn note(self) -> &'static str {
        match self {
            Self::History => "Older scrollback was not kept.",
            Self::Screen => "The screen was outside the geometry or the 4 MiB bound.",
        }
    }

    pub(crate) fn from_kept(screen_kept: bool, history_dropped: u32) -> Option<Self> {
        if !screen_kept {
            Some(Self::Screen)
        } else if history_dropped > 0 {
            Some(Self::History)
        } else {
            None
        }
    }
}
