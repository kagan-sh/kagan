//! An owned process tree, and the Windows job object that ends it.

mod capture;
mod path;
mod process;

#[cfg(windows)]
mod job;

#[cfg(unix)]
mod tree;

#[cfg(test)]
mod tests;

pub use capture::{Captured, capture, capture_with_input};
pub use path::{find_program, pathext, user_path, warm_user_path};
pub use process::Process;

#[cfg(windows)]
pub use job::Job;
#[cfg(unix)]
pub use tree::{kill_leader, kill_tree, pid_of};
