//! An owned process tree, and the Windows job object that ends it.

mod capture;
mod path;
mod process;

#[cfg(windows)]
mod job;

#[cfg(test)]
mod tests;

pub use capture::{Captured, capture, capture_with_limit};
pub use path::{find_program, pathext, user_path, warm_user_path};
pub use process::Process;

#[cfg(windows)]
pub use job::Job;
