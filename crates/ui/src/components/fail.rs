#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub(crate) struct ComponentError(String);

impl ComponentError {
    pub(crate) fn new(message: impl std::fmt::Display) -> Self {
        Self(message.to_string())
    }
}

macro_rules! fail {
    ($($arg:tt)*) => {
        return Err($crate::components::fail::ComponentError::new(format!($($arg)*)).into())
    };
}

macro_rules! ensure {
    ($cond:expr, $($arg:tt)*) => {
        if !$cond {
            fail!($($arg)*);
        }
    };
}
