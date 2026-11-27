use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, App, Entity, Global, InteractiveElement as _, IntoElement, ParentElement as _,
    Styled as _, WeakEntity, div, px,
};
use kg_domain::ThreadId;

use super::ChatView;
use super::composer::COLUMN;
use crate::kagan::Kagan;
use crate::kagan::Shown;
use crate::kagan::screen::Center;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

// Where a chat is drawn. The center's chat owns the window's keyboard; an embed never takes it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Place {
    Center,
    Embedded,
}

// The embedded chats on screen, oldest first. The center's chat is not here: the screen owns it.
#[derive(Default)]
struct Embeds {
    chats: Vec<(ThreadId, WeakEntity<ChatView>)>,
}

impl Global for Embeds {}

pub(super) fn register(thread: &ThreadId, chat: WeakEntity<ChatView>, cx: &mut App) {
    let embeds = cx.default_global::<Embeds>();
    embeds.chats.retain(|(_, seen)| seen.upgrade().is_some());
    embeds.chats.push((thread.clone(), chat));
}

// The box that answers a draft is that thread's one box on screen: the center's, or the first
// embed for it.
pub(crate) fn boxed(thread: &ThreadId, cx: &App) -> Option<Entity<ChatView>> {
    cx.try_global::<Embeds>()?
        .chats
        .iter()
        .filter(|(seen, _)| seen == thread)
        .find_map(|(_, chat)| chat.upgrade())
}

// Whether the center draws this thread's box, so the embed draws the way to it instead of a
// second box.
pub(super) fn answered_elsewhere(chat: &ChatView, cx: &App) -> bool {
    if chat.place != Place::Embedded {
        return false;
    }
    chat.kagan.upgrade().is_some_and(|kagan| {
        let kagan = kagan.read(cx);
        matches!(
            kagan.screen.current(),
            Center::Work(Shown::Chat(view)) if view.read(cx).thread() == &chat.thread
        )
    })
}

pub(super) fn reply_in_center(kagan: &WeakEntity<Kagan>) -> AnyElement {
    let kagan = kagan.clone();
    v_flex()
        .flex_none()
        .px_4()
        .pb_4()
        .child(
            div().max_w(px(COLUMN)).mx_auto().w_full().child(
                div()
                    .id("chat-reply-in-center")
                    .ui_observe_as(Facts::button("Reply in the center"))
                    .child(
                        Button::new("chat-reply-in-center-button")
                            .small()
                            .outline()
                            .label("Reply in the center")
                            .tooltip("Focus the prompt box in the center")
                            .on_click(move |_, window, cx| {
                                _ = kagan.update(cx, |kagan, cx| kagan.focus_work(window, cx));
                            }),
                    ),
            ),
        )
        .into_any_element()
}
