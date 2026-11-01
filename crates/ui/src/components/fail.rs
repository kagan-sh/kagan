#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub(crate) struct ComponentError(String);

impl ComponentError {
    pub(crate) fn new(message: impl std::fmt::Display) -> Self {
        Self(message.to_string())
    }
}
