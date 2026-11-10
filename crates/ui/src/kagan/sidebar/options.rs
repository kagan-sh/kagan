use std::cmp::Ordering;

use kg_domain::{ProjectId, SectionId, Thread, newest_first};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(super) enum ThreadOrder {
    #[default]
    Current,
    Attention,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(super) struct ListOptions {
    pub(super) order: ThreadOrder,
    pub(super) hide_empty_projects: bool,
    pub(super) hide_empty_sections: bool,
    pub(super) unread_first: bool,
}

impl ListOptions {
    pub(super) fn from_saved(
        order: Option<&str>,
        hide_empty_projects: bool,
        hide_empty_sections: bool,
        unread_first: bool,
    ) -> Self {
        let order = if order == Some(kg_domain::prefs::SIDEBAR_ORDER_ATTENTION) {
            ThreadOrder::Attention
        } else {
            ThreadOrder::Current
        };
        Self {
            order,
            hide_empty_projects,
            hide_empty_sections,
            unread_first,
        }
    }
}

pub(super) fn sort_threads(threads: &mut [&Thread], options: ListOptions) {
    threads.sort_by(|left, right| {
        band(left, options)
            .cmp(&band(right, options))
            .then_with(|| match options.order {
                ThreadOrder::Current => newest_first(left, right),
                ThreadOrder::Attention => attention_first(left, right),
            })
    });
}

// Pinned threads are the first band. Unread is the next band only when that option is on.
fn band(thread: &Thread, options: ListOptions) -> u8 {
    if thread.pinned {
        0
    } else if options.unread_first && thread.unread() {
        1
    } else {
        2
    }
}

fn attention_first(left: &Thread, right: &Thread) -> Ordering {
    right
        .latest_attention_at
        .cmp(&left.latest_attention_at)
        .then_with(|| right.id.cmp(&left.id))
}

pub(super) fn section_row_id(project: Option<&ProjectId>, section: &SectionId) -> String {
    match project {
        Some(project) => format!("section-{project}-{section}"),
        None => format!("section-{section}"),
    }
}
