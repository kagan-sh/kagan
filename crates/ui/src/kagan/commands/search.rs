use std::rc::Rc;

use crate::theme::TypeScale as _;
use gpui_kit::component::command::{Command, CommandGroup, CommandItem};
use gpui_kit::component::{ActiveTheme as _, IndexPath, h_flex, v_flex};
use gpui_kit::{
    Context, ElementId, InteractiveElement as _, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Task, WeakEntity, Window, div,
};
use kg_core::Workspace;
use kg_domain::{Found, Op, OpResult, SEARCH_DEFAULT, ThreadOp};

use super::CommandList;
use crate::kagan::Kagan;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

impl Kagan {
    fn open_found(&mut self, found: Found, window: &mut Window, cx: &mut Context<Self>) {
        if self.close_commands(window, cx).is_none() {
            return;
        }
        let id = found.thread_id;
        self.select(id.clone(), window, cx);
        let Some(snippet) = found.snippet else {
            return;
        };
        // The host keeps where a view was scrolled, so it goes to the bottom first.
        let bottom = Workspace::dispatch(
            Op::Thread(ThreadOp::Scroll {
                id: id.clone(),
                lines: -10_000,
                terminal: None,
            }),
            cx,
        );
        cx.spawn(async move |_, cx| {
            if bottom.await.is_ok() {
                let lines = i32::try_from(snippet.scroll).unwrap_or(i32::MAX);
                let back = cx.update(|cx| {
                    Workspace::dispatch(
                        Op::Thread(ThreadOp::Scroll {
                            id,
                            lines,
                            terminal: None,
                        }),
                        cx,
                    )
                });
                _ = back.await;
            }
        })
        .detach();
    }

    fn find_threads(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(list) = self.commands.as_mut() else {
            return;
        };
        list.found.clear();
        list.failed = None;
        if query.trim().is_empty() {
            list.search = Task::ready(());
            list.state
                .update(cx, |state, cx| state.set_loading(false, window, cx));
            cx.notify();
            return;
        }
        list.state
            .update(cx, |state, cx| state.set_loading(true, window, cx));
        let task = Workspace::dispatch(
            Op::Thread(ThreadOp::Search {
                text: query.to_owned(),
                limit: Some(SEARCH_DEFAULT),
                archived: true,
            }),
            cx,
        );
        // Replacing the task drops the search it waits on, so an older query stops there.
        list.search = cx.spawn_in(window, async move |this, cx| {
            let (found, failed) = match task.await {
                Ok(OpResult::Found(found)) => (found, None),
                Ok(_) => (
                    Vec::new(),
                    Some("Search gave an unexpected answer.".to_owned()),
                ),
                Err(error) => (Vec::new(), Some(format!("Search failed: {error}"))),
            };
            _ = this.update_in(cx, |this, window, cx| {
                if let Some(list) = this.commands.as_mut() {
                    list.found = found;
                    list.failed = failed.map(SharedString::from);
                    list.state
                        .update(cx, |state, cx| state.set_loading(false, window, cx));
                    cx.notify();
                }
            });
        });
        cx.notify();
    }
}

pub(super) fn threads(list: &CommandList, this: WeakEntity<Kagan>) -> Command {
    let (live, archived): (Vec<&Found>, Vec<&Found>) =
        list.found.iter().partition(|found| !found.archived);
    let opened: Rc<Vec<Vec<Found>>> = Rc::new(
        [&live, &archived]
            .into_iter()
            .map(|group| group.iter().map(|found| (*found).clone()).collect())
            .collect(),
    );
    let searched = this.clone();
    let empty = list
        .failed
        .clone()
        .unwrap_or_else(|| SharedString::from("No threads match."));
    let mut command = Command::new(&list.state)
        .filterable(false)
        .placeholder("Search threads…")
        .empty(move |_, _, cx| {
            div()
                .id("command-threads-empty")
                .ui_container()
                .aria_label(empty.clone())
                .p_3()
                .text_color(cx.theme().muted_foreground)
                .child(empty.clone())
        })
        .on_query(move |query, window, cx| {
            _ = searched.update(cx, |this, cx| this.find_threads(query, window, cx));
        })
        .on_confirm(move |index: IndexPath, window, cx| {
            let Some(found) = opened
                .get(index.section)
                .and_then(|group| group.get(index.row))
                .cloned()
            else {
                return;
            };
            _ = this.update(cx, |this, cx| this.open_found(found, window, cx));
        });
    for (title, group) in [("Threads", live), ("Archived", archived)] {
        command = command.group(
            CommandGroup::new()
                .label(title)
                .items(group.into_iter().cloned().map(found_item)),
        );
    }
    command
}

fn found_item(found: Found) -> CommandItem {
    CommandItem::new()
        .label(found.title.clone())
        .child(move |_, cx| {
            let muted = cx.theme().muted_foreground;
            v_flex()
                .id(ElementId::Name(
                    format!("found-{}", found.thread_id.0).into(),
                ))
                .ui_observe_as(Facts {
                    value: found
                        .snippet
                        .as_ref()
                        .map(|snippet| snippet.text.clone().into()),
                    ..Facts::button(found.title.clone())
                })
                .w_full()
                .min_w_0()
                .child(
                    h_flex()
                        .gap_2()
                        .child(div().text_ellipsis().child(found.title.clone()))
                        .child(
                            div()
                                .type_small(cx)
                                .text_color(muted)
                                .child(found.project.clone()),
                        ),
                )
                .children(found.snippet.as_ref().map(|snippet| {
                    div()
                        .type_small(cx)
                        .text_color(muted)
                        .text_ellipsis()
                        .child(snippet.text.clone())
                }))
        })
}

#[cfg(test)]
#[cfg(unix)]
pub(crate) fn found(kagan: &Kagan) -> Option<Vec<(kg_domain::ThreadId, bool)>> {
    kagan.commands.as_ref().map(|list| {
        list.found
            .iter()
            .map(|found| (found.thread_id.clone(), found.archived))
            .collect()
    })
}
