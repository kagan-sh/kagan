use crate::theme::TypeScale as _;
use gpui_kit::FocusHandle;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, Focusable as _, InteractiveElement as _,
    IntoElement, ParentElement as _, Role, Styled as _, Subscription, Window, div,
};
use kg_domain::ThreadId;

use super::Kagan;
use super::controls::control;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

use std::time::Instant;

use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::h_flex;

use crate::web_page::WebPage;

pub(super) mod act;
pub(crate) mod agent;
mod chrome;
pub(super) mod snapshot;

pub(super) struct Agent {
    pub(super) thread: ThreadId,
    pub(super) url: String,
    pub(super) used: Instant,
}

pub(super) struct Browser {
    address: Entity<InputState>,
    page: Option<Entity<WebPage>>,
    // Whether the page was last told to show. Tests read it, since they build no native page.
    pub(super) shown: bool,
    pub(super) agent: Option<Agent>,
    driven_focus: FocusHandle,
    follow: u64,
    note: Option<String>,
    _enter: Subscription,
}

impl Browser {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Kagan>) -> Self {
        let address = cx
            .new(|cx| InputState::new(window, cx).placeholder("Web address, such as example.com"));
        let enter = cx.subscribe_in(&address, window, |this, _, event, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.open_address(window, cx);
            }
        });
        Self {
            address,
            page: None,
            shown: false,
            agent: None,
            driven_focus: cx.focus_handle().tab_stop(true),
            follow: 0,
            note: None,
            _enter: enter,
        }
    }

    fn open(&mut self, url: &url::Url, window: &mut Window, cx: &mut Context<Kagan>) {
        match self.page_for(url, window, cx) {
            Ok(page) => self.page = Some(page),
            Err(error) => self.note = Some(error),
        }
    }

    pub(super) fn page_for(
        &mut self,
        url: &url::Url,
        window: &mut Window,
        cx: &mut Context<Kagan>,
    ) -> Result<Entity<WebPage>, String> {
        if let Some(page) = &self.page {
            page.update(cx, |page, cx| page.load_url(url.as_str(), cx));
            return Ok(page.clone());
        }
        let url = url.as_str().to_owned();
        #[cfg(target_os = "linux")]
        let again = url.clone();
        let page = crate::web_page::open(window, cx, move |builder| {
            builder
                .with_url(&url)
                .with_navigation_handler(|target| is_web(&target))
        })?;
        #[cfg(target_os = "linux")]
        Self::load_sized(page.clone(), again, window, cx);
        Ok(page)
    }
    // The view is built at empty bounds. WebKitGTK drops a navigation that starts there.
    #[cfg(target_os = "linux")]
    fn load_sized(page: Entity<WebPage>, url: String, window: &mut Window, cx: &mut App) {
        window
            .spawn(cx, async move |cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(80))
                    .await;
                cx.update(|_, cx| page.update(cx, |page, cx| page.load_url(&url, cx)))
                    .ok(); // The app is gone; there is no page to load.
            })
            .detach();
    }

    pub(super) fn driven(&self) -> bool {
        self.agent.is_some()
    }

    pub(super) fn owner(&self) -> Option<&ThreadId> {
        self.agent.as_ref().map(|agent| &agent.thread)
    }

    fn sync_page(&mut self, shown: bool, cx: &mut App) {
        self.shown = shown;
        let Some(page) = &self.page else { return };
        if page.read(cx).visible(cx) != shown {
            page.update(cx, |page, cx| {
                if shown {
                    page.show(cx);
                } else {
                    page.hide(cx);
                }
            });
        }
    }

    // Closing the tab ends the drive. The thread's page goes with it.
    pub(super) fn close(&mut self, window: &mut Window, cx: &mut App) {
        {
            self.sync_page(false, cx);
            self.page = None;
            self.agent = None;
            self.follow = self.follow.wrapping_add(1);
        }
        self.note = None;
        self.address.update(cx, |address, cx| {
            address.set_value(String::new(), window, cx);
        });
    }

    pub(super) fn leave_thread(&mut self, window: &mut Window, cx: &mut App) {
        {
            self.sync_page(false, cx);
            self.follow = self.follow.wrapping_add(1);
            if self.agent.is_none() {
                self.page = None;
            }
        }
        self.note = None;
        self.address.update(cx, |address, cx| {
            address.set_value(String::new(), window, cx);
        });
    }

    #[cfg(test)]
    #[cfg(unix)]
    pub(super) fn address(&self, cx: &App) -> String {
        self.address.read(cx).value().to_string()
    }

    pub(super) fn holds_focus(&self, window: &Window, cx: &App) -> bool {
        if self.address.focus_handle(cx).contains_focused(window, cx) {
            return true;
        }
        self.driven() && self.driven_focus.contains_focused(window, cx)
    }

    pub(super) fn focus(&self, window: &mut Window, cx: &mut App) {
        if self.driven() {
            window.focus(&self.driven_focus, cx);
            return;
        }
        window.focus(&self.address.focus_handle(cx), cx);
    }
}

pub(super) fn page_url(typed: &str) -> Option<url::Url> {
    let typed = typed.trim();
    let text = if typed.contains("://") {
        typed.to_owned()
    } else if super::tabs::local_host(&format!("http://{typed}")).is_some() {
        format!("http://{typed}")
    } else {
        format!("https://{typed}")
    };
    url::Url::parse(&text)
        .ok()
        .filter(|url| is_web(url.as_str()))
}

const HINT: &str = "Type an address and press Enter.";

pub(super) fn is_web(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
}

pub(super) fn secure(typed: &str) -> bool {
    page_url(typed).is_some_and(|url| url.scheme() == "https")
}

impl Kagan {
    pub(super) fn browser_drawn(&self, cx: &App) -> bool {
        self.right_drawn()
            && super::right::selected_tab(self, cx) == Some(super::right::Tab::Browser)
            && self.held_elsewhere(cx).is_none()
    }

    // The thread driving the browser, when the thread on screen is not that one. Its page is not
    // this thread's to show or to navigate.
    pub(super) fn held_elsewhere(&self, cx: &App) -> Option<ThreadId> {
        let owner = self.browser.owner()?;
        (self.shown_thread(cx).as_ref() != Some(owner)).then(|| owner.clone())
    }

    pub(super) fn sync_browser(&mut self, cx: &mut App) {
        let shown = self.browser_drawn(cx);
        self.browser.sync_page(shown, cx);
    }

    // Hides the page in the step that stops drawing it. Showing waits for the render, which places
    // the page at its final bounds.
    pub(super) fn hide_browser(&mut self, cx: &mut App) {
        if !self.browser_drawn(cx) {
            self.browser.sync_page(false, cx);
        }
    }

    pub(super) fn stop_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_agent(window, cx);
        self.sync_browser(cx);
        self.focus_right_tab(window, cx);
        cx.notify();
    }

    pub(super) fn render_browser(&self, cx: &mut Context<Self>) -> AnyElement {
        if let Some(owner) = self.held_elsewhere(cx) {
            return self.render_held(&owner, cx);
        }
        if self.browser.agent.is_some() {
            return self.render_driven(cx);
        }
        let note = self.browser.note.clone().map(|note| {
            div()
                .id("browser-note")
                .ui_observe_as(Facts {
                    role: Role::Label,
                    ..Facts::button(note.clone())
                })
                .type_small(cx)
                .text_color(cx.theme().danger)
                .child(note)
        });
        let page = self.browser.page.clone();
        v_flex()
            .id("browser")
            .ui_container()
            .size_full()
            .gap_2()
            .p_2()
            .child(self.address_row(cx))
            .children(note)
            .child(match page {
                Some(page) => div().flex_1().min_h_0().child(page).into_any_element(),
                None => div()
                    .id("browser-hint")
                    .ui_observe_as(Facts {
                        role: Role::Label,
                        ..Facts::button(HINT)
                    })
                    .type_small(cx)
                    .child(HINT)
                    .into_any_element(),
            })
            .into_any_element()
    }

    fn render_held(&self, owner: &ThreadId, cx: &mut Context<Self>) -> AnyElement {
        let thread = self
            .record(owner, cx)
            .map_or_else(|| owner.to_string(), |thread| thread.title.clone());
        let text =
            format!("Thread {thread} is driving the browser. Stop its agent there to use it here.");
        v_flex()
            .id("browser")
            .ui_container()
            .size_full()
            .p_2()
            .child(
                div()
                    .id("browser-held")
                    .ui_observe_as(Facts {
                        role: Role::Label,
                        ..Facts::button(text.clone())
                    })
                    .type_small(cx)
                    .text_color(cx.theme().muted_foreground)
                    .child(text),
            )
            .into_any_element()
    }

    fn render_driven(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(agent) = &self.browser.agent else {
            return div().into_any_element();
        };
        let thread = self
            .record(&agent.thread, cx)
            .map_or_else(|| agent.thread.to_string(), |thread| thread.title.clone());
        let row = h_flex()
            .id("browser-driven")
            .flex_none()
            .gap_1()
            .px_2()
            .pt_2()
            .items_center()
            .track_focus(&self.browser.driven_focus)
            .tab_stop(true)
            .child(self.browser_nav(cx))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .type_small(cx)
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("Driven by {thread}")),
            )
            .child(
                div()
                    .id("browser-driven-url")
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .type_small(cx)
                    .text_color(cx.theme().muted_foreground)
                    .child(self.live_url(cx)),
            )
            .when(secure(&self.live_url(cx)), |row| {
                row.child(Self::secure_mark(cx))
            })
            .child(self.external_button(cx))
            .child(
                div().flex_none().child(control(
                    "browser-stop-agent",
                    Facts::button("Stop agent"),
                    Button::new("browser-stop-agent-button")
                        .small()
                        .outline()
                        .label("Stop agent")
                        .on_click(cx.listener(|this, _, window, cx| this.stop_agent(window, cx))),
                )),
            );
        let page = self.browser.page.clone();
        v_flex()
            .id("browser")
            .ui_container()
            .size_full()
            .min_h_0()
            .child(row)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .px_2()
                    .pb_2()
                    .child(
                        div()
                            .id("browser-page")
                            .size_full()
                            .children(page)
                            .ui_observe_as(Facts {
                                role: Role::Unknown,
                                ..Facts::button("Browser page")
                            }),
                    )
                    .into_any_element(),
            )
            .into_any_element()
    }

    // A terminal link opened in the Browser tab. A link that is not a web address, or a page that
    // cannot open, goes to the system browser.
    pub(crate) fn open_link(&mut self, link: &str, window: &mut Window, cx: &mut Context<Self>) {
        let LinkTarget::Tab(url) = link_target(link, true) else {
            cx.open_url(link);
            return;
        };
        if self.held_elsewhere(cx).is_some() {
            cx.open_url(link);
            return;
        }
        #[cfg(test)]
        TAB_OPENS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let Ok(page) = self.browser.page_for(&url, window, cx) else {
            cx.open_url(link);
            return;
        };
        self.browser.page = Some(page.clone());
        self.open_browser(window, cx);
        self.follow_address(page, cx);
        cx.notify();
    }
    fn open_address(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.browser.address.read(cx).value().to_string();
        match page_url(&typed) {
            Some(url) => {
                self.browser.note = None;
                if let Some(page) = self.browser.page.clone() {
                    page.read(cx).arm();
                }
                self.browser.open(&url, window, cx);
                if self.browser.note.is_none()
                    && let Some(page) = self.browser.page.clone()
                {
                    self.follow_address(page, cx);
                }
            }
            None => {
                self.browser.note = Some("Type a web address, such as example.com.".into());
            }
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests;

/// How many links were sent to the Browser tab; the tests read it to tell the switch apart.
#[cfg(test)]
pub(crate) static TAB_OPENS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LinkTarget {
    System,
    Tab(url::Url),
}

pub(crate) fn link_target(link: &str, in_tab: bool) -> LinkTarget {
    if !in_tab {
        return LinkTarget::System;
    }
    url::Url::parse(link)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https"))
        .map_or(LinkTarget::System, LinkTarget::Tab)
}

#[cfg(test)]
mod link_tests {
    use super::{LinkTarget, link_target};

    #[test]
    fn a_web_link_goes_to_the_tab_when_the_switch_is_on() {
        assert!(matches!(
            link_target("https://example.com/x", true),
            LinkTarget::Tab(_)
        ));
        assert!(matches!(
            link_target("http://localhost:4321/x", true),
            LinkTarget::Tab(_)
        ));
    }

    #[test]
    fn other_schemes_never_reach_the_tab() {
        for link in [
            "file:///etc/passwd",
            "mailto:someone@example.com",
            "javascript:alert(1)",
            "ssh://host/repo",
            "not a link",
        ] {
            assert_eq!(link_target(link, true), LinkTarget::System, "{link}");
        }
    }

    #[test]
    fn the_switch_off_keeps_every_link_in_the_system_browser() {
        assert_eq!(
            link_target("https://example.com/x", false),
            LinkTarget::System
        );
    }
}
