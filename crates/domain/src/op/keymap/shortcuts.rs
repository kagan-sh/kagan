/// Where a shortcut is bound. Sending a review also works inside its inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShortcutScope {
    Window,
    Review,
    ReviewSend,
}

/// What must be open to run a shortcut from the command list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShortcutWhen {
    Always,
    Thread,
    NewThread,
    Review,
}

/// The heading under which a built-in appears in the command list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandGroup {
    Threads,
    NewThread,
    Review,
    Window,
    Settings,
}

impl CommandGroup {
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::Threads => "Threads",
            Self::NewThread => "New thread",
            Self::Review => "Review",
            Self::Window => "Window",
            Self::Settings => "Settings",
        }
    }
}

/// The Keyboard heading, independent of the command-list heading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShortcutGroup {
    Window,
    Thread,
    NewThread,
    Review,
}

/// One shortcut: the saved name, the GPUI action it binds, the default chord, and the label.
#[derive(Debug, PartialEq, Eq)]
pub struct Shortcut {
    pub name: &'static str,
    pub action: &'static str,
    pub chord: &'static str,
    pub label: &'static str,
    pub scope: ShortcutScope,
    pub when: ShortcutWhen,
    pub command_group: CommandGroup,
    pub settings_group: ShortcutGroup,
}

// Built-in shortcuts. A saved override uses `name`, so those names stay.
const fn bind(
    name: &'static str,
    action: &'static str,
    chord: &'static str,
    label: &'static str,
    place: (ShortcutScope, ShortcutWhen, CommandGroup, ShortcutGroup),
) -> Shortcut {
    let (scope, when, command_group, settings_group) = place;
    Shortcut {
        name,
        action,
        chord,
        label,
        scope,
        when,
        command_group,
        settings_group,
    }
}

#[rustfmt::skip]
pub const SHORTCUTS: &[Shortcut] = &[
    bind("Bigger", "kagan::Bigger", "secondary-=", "Bigger terminal text", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Window, ShortcutGroup::Window)),
    bind("Smaller", "kagan::Smaller", "secondary--", "Smaller terminal text", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Window, ShortcutGroup::Window)),
    bind("ActualSize", "kagan::ActualSize", "secondary-0", "Actual terminal text size", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Window, ShortcutGroup::Window)),
    bind("NewThread", "kagan::NewThread", "secondary-n", "New thread", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Threads, ShortcutGroup::Window)),
    bind("StopThread", "kagan::StopThread", "secondary-.", "Stop thread", (ShortcutScope::Window, ShortcutWhen::Thread, CommandGroup::Threads, ShortcutGroup::Thread)),
    bind("ArchiveThread", "kagan::ArchiveThread", "secondary-shift-backspace", "Archive thread", (ShortcutScope::Window, ShortcutWhen::Thread, CommandGroup::Threads, ShortcutGroup::Thread)),
    bind("TogglePinned", "kagan::TogglePinned", "secondary-shift-p", "Pin or unpin thread", (ShortcutScope::Window, ShortcutWhen::Thread, CommandGroup::Threads, ShortcutGroup::Thread)),
    bind("NextThread", "kagan::NextThread", "secondary-shift-]", "Next thread", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Threads, ShortcutGroup::Window)),
    bind("PreviousThread", "kagan::PreviousThread", "secondary-shift-[", "Previous thread", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Threads, ShortcutGroup::Window)),
    bind("NextThreadNeedingYou", "kagan::NextThreadNeedingYou", "secondary-alt-j", "Next thread needing you", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Threads, ShortcutGroup::Window)),
    bind("Thread1", "kagan::Thread1", "secondary-1", "Thread 1", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Threads, ShortcutGroup::Window)),
    bind("Thread2", "kagan::Thread2", "secondary-2", "Thread 2", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Threads, ShortcutGroup::Window)),
    bind("Thread3", "kagan::Thread3", "secondary-3", "Thread 3", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Threads, ShortcutGroup::Window)),
    bind("Thread4", "kagan::Thread4", "secondary-4", "Thread 4", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Threads, ShortcutGroup::Window)),
    bind("Thread5", "kagan::Thread5", "secondary-5", "Thread 5", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Threads, ShortcutGroup::Window)),
    bind("Thread6", "kagan::Thread6", "secondary-6", "Thread 6", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Threads, ShortcutGroup::Window)),
    bind("Thread7", "kagan::Thread7", "secondary-7", "Thread 7", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Threads, ShortcutGroup::Window)),
    bind("Thread8", "kagan::Thread8", "secondary-8", "Thread 8", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Threads, ShortcutGroup::Window)),
    bind("Thread9", "kagan::Thread9", "secondary-9", "Thread 9", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Threads, ShortcutGroup::Window)),
    bind("OpenSettings", "kagan::OpenSettings", "secondary-,", "Settings", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Settings, ShortcutGroup::Window)),
    bind("OpenSkills", "kagan::OpenSkills", "secondary-alt-k", "Skills", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Window, ShortcutGroup::Window)),
    bind("UseProjectFolder", "kagan::UseProjectFolder", "secondary-alt-p", "Project folder", (ShortcutScope::Window, ShortcutWhen::NewThread, CommandGroup::NewThread, ShortcutGroup::NewThread)),
    bind("UseWorktreeFolder", "kagan::UseWorktreeFolder", "secondary-alt-w", "New worktree", (ShortcutScope::Window, ShortcutWhen::NewThread, CommandGroup::NewThread, ShortcutGroup::NewThread)),
    bind("RetryBranches", "kagan::RetryBranches", "secondary-alt-r", "Retry branches", (ShortcutScope::Window, ShortcutWhen::NewThread, CommandGroup::NewThread, ShortcutGroup::NewThread)),
    bind("RetryPrograms", "kagan::RetryPrograms", "secondary-alt-l", "Retry programs", (ShortcutScope::Window, ShortcutWhen::NewThread, CommandGroup::NewThread, ShortcutGroup::NewThread)),
    bind("ToggleCustomCommand", "kagan::ToggleCustomCommand", "secondary-alt-c", "Custom command", (ShortcutScope::Window, ShortcutWhen::NewThread, CommandGroup::NewThread, ShortcutGroup::NewThread)),
    bind("ThreadActions", "kagan::ThreadActions", "secondary-shift-m", "Thread actions", (ShortcutScope::Window, ShortcutWhen::Thread, CommandGroup::Threads, ShortcutGroup::Thread)),
    bind("ToggleSidebar", "kagan::ToggleSidebar", "secondary-b", "Show or hide left sidebar", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Window, ShortcutGroup::Window)),
    bind("ToggleRightSidebar", "kagan::ToggleRightSidebar", "secondary-\\", "Show or hide right sidebar", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Window, ShortcutGroup::Window)),
    bind("ToggleActivityLog", "kagan::ToggleActivityLog", "secondary-shift-e", "Show or hide activity log", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Window, ShortcutGroup::Window)),
    bind("AddProject", "kagan::AddProject", "secondary-shift-o", "Add project", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Window, ShortcutGroup::Window)),
    bind("SendReview", "review::SendReview", "secondary-enter", "Send review", (ShortcutScope::ReviewSend, ShortcutWhen::Review, CommandGroup::Review, ShortcutGroup::Review)),
    bind("NextHunk", "review::NextChange", "alt-]", "Next change", (ShortcutScope::Review, ShortcutWhen::Review, CommandGroup::Review, ShortcutGroup::Review)),
    bind("PrevHunk", "review::PrevChange", "alt-[", "Previous change", (ShortcutScope::Review, ShortcutWhen::Review, CommandGroup::Review, ShortcutGroup::Review)),
    bind("FindInDiff", "review::FindInDiff", "secondary-f", "Find in diff", (ShortcutScope::Review, ShortcutWhen::Review, CommandGroup::Review, ShortcutGroup::Review)),
    bind("ToggleWrap", "review::ToggleWrap", "alt-z", "Wrap lines", (ShortcutScope::Review, ShortcutWhen::Review, CommandGroup::Review, ShortcutGroup::Review)),
    bind("NextMatch", "review::NextMatch", "secondary-g", "Next match", (ShortcutScope::Review, ShortcutWhen::Review, CommandGroup::Review, ShortcutGroup::Review)),
    bind("PrevMatch", "review::PrevMatch", "secondary-alt-g", "Previous match", (ShortcutScope::Review, ShortcutWhen::Review, CommandGroup::Review, ShortcutGroup::Review)),
    bind("StartAgain", "kagan::StartAgain", "secondary-shift-s", "Start again", (ShortcutScope::Window, ShortcutWhen::Thread, CommandGroup::Threads, ShortcutGroup::Thread)),
    bind("FocusSidebar", "kagan::FocusSidebar", "secondary-shift-l", "Focus sidebar", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Window, ShortcutGroup::Window)),
    bind("RestoreThread", "kagan::RestoreThread", "secondary-shift-r", "Restore thread", (ShortcutScope::Window, ShortcutWhen::Thread, CommandGroup::Threads, ShortcutGroup::Thread)),
    bind("UseBuiltInSidebar", "kagan::UseBuiltInSidebar", "secondary-alt-b", "Use built-in sidebar", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Settings, ShortcutGroup::Window)),
    bind("UseBuiltInReview", "kagan::UseBuiltInReview", "secondary-alt-v", "Use built-in review", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Settings, ShortcutGroup::Window)),
    bind("OpenCommandList", "kagan::OpenCommandList", "secondary-shift-a", "Command list", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Threads, ShortcutGroup::Window)),
    bind("SearchThreads", "kagan::SearchThreads", "secondary-alt-f", "Search threads…", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Threads, ShortcutGroup::Window)),
    bind("NewTerminal", "kagan::NewTerminal", "secondary-t", "New terminal", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Threads, ShortcutGroup::Window)),
    bind("CloseTerminal", "kagan::CloseTerminal", "secondary-shift-w", "Close terminal", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Threads, ShortcutGroup::Window)),
    bind("NextTerminal", "kagan::NextTerminal", "secondary-alt-]", "Next terminal", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Threads, ShortcutGroup::Window)),
    bind("PreviousTerminal", "kagan::PreviousTerminal", "secondary-alt-[", "Previous terminal", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Threads, ShortcutGroup::Window)),
    bind("OpenLocalLink", "kagan::OpenLocalLink", "secondary-alt-o", "Open local link", (ShortcutScope::Window, ShortcutWhen::Always, CommandGroup::Threads, ShortcutGroup::Window)),
];
