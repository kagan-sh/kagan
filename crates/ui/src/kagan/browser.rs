use crate::theme::TypeScale as _;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use gpui_kit::FocusHandle;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, Focusable as _, InteractiveElement as _,
    IntoElement, ParentElement as _, Role, Styled as _, Subscription, Window, div,
};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use kg_domain::ThreadId;

use super::Kagan;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use super::controls::control;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::time::Instant;

#[cfg(any(target_os = "macos", target_os = "windows"))]
use gpui_kit::component::Sizable as _;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use gpui_kit::component::button::Button;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use gpui_kit::component::h_flex;

#[cfg(any(target_os = "macos", target_os = "windows"))]
use crate::web_page::WebPage;

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) mod agent;
#[cfg(any(test, target_os = "macos", target_os = "windows"))]
pub(super) mod snapshot;

/// A thread's hold on the Browser's one page, while it drives it.
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(super) struct Agent {
    pub(super) thread: ThreadId,
    /// The last URL the page reported.
    pub(super) url: String,
    /// The last command that reached it, for the 30-minute rule.
    pub(super) used: Instant,
}

pub(super) struct Browser {
    address: Entity<InputState>,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    page: Option<Entity<WebPage>>,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub(super) agent: Option<Agent>,
    /// Where the keys go while a thread drives the page, since the address field is hidden.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    driven_focus: FocusHandle,
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
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            page: None,
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            agent: None,
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            driven_focus: cx.focus_handle().tab_stop(true),
            note: None,
            _enter: enter,
        }
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn open(&mut self, url: &url::Url, window: &mut Window, cx: &mut Context<Kagan>) {
        match self.page_for(url, window, cx) {
            Ok(page) => self.page = Some(page),
            Err(error) => self.note = Some(error),
        }
    }

    /// Loads `url` in the person's page, or makes one when there is none.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
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
        // Links and redirects inside the page are held to the same rule as the address field.
        // Private: cookies and logins end with the page, and nothing lands outside kagan's data.
        let url = url.as_str().to_owned();
        crate::web_page::open(window, cx, move |builder| {
            builder
                .with_url(&url)
                .with_navigation_handler(|target| is_web(&target))
        })
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    fn open(&mut self, _: &url::Url, _: &mut Window, _: &mut Context<Kagan>) {
        self.note = Some(UNSUPPORTED.into());
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub(super) fn driven(&self) -> bool {
        self.agent.is_some()
    }

    /// The thread that drives the page, while one does.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub(super) fn owner(&self) -> Option<&ThreadId> {
        self.agent.as_ref().map(|agent| &agent.thread)
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn sync_page(&self, shown: bool, cx: &mut App) {
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

    /// Drops the page and its address, so nothing on it keeps running once its tab is gone.
    /// A thread's page goes with it: closing the tab ends the drive.
    pub(super) fn close(&mut self, window: &mut Window, cx: &mut App) {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            self.page = None;
            self.agent = None;
        }
        self.note = None;
        self.address.update(cx, |address, cx| {
            address.set_value(String::new(), window, cx);
        });
    }

    /// Showing another thread drops the person's page. A thread's page stays loaded and hidden.
    pub(super) fn leave_thread(&mut self, window: &mut Window, cx: &mut App) {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            if self.agent.is_none() {
                self.page = None;
            }
        }
        self.note = None;
        self.address.update(cx, |address, cx| {
            address.set_value(String::new(), window, cx);
        });
    }

    #[cfg(all(test, any(target_os = "macos", target_os = "windows")))]
    pub(super) fn address(&self, cx: &App) -> String {
        self.address.read(cx).value().to_string()
    }

    pub(super) fn holds_focus(&self, window: &Window, cx: &App) -> bool {
        if self.address.focus_handle(cx).contains_focused(window, cx) {
            return true;
        }
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            self.driven() && self.driven_focus.contains_focused(window, cx)
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            false
        }
    }

    pub(super) fn focus(&self, window: &mut Window, cx: &mut App) {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        if self.driven() {
            window.focus(&self.driven_focus, cx);
            return;
        }
        window.focus(&self.address.focus_handle(cx), cx);
    }
}

const WEB: bool = cfg!(any(target_os = "macos", target_os = "windows"));
pub(crate) const UNSUPPORTED: &str = "The browser works on macOS and Windows only for now.";

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

fn is_web(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
}

impl Kagan {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub(super) fn browser_drawn(&self, cx: &App) -> bool {
        self.right_drawn()
            && super::right::selected_tab(self, cx).as_deref() == Some(super::tabs::BROWSER)
    }

    /// Hides or shows the native page to match whether the Browser tab is on screen.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub(super) fn sync_browser(&self, cx: &mut App) {
        let shown = self.browser_drawn(cx);
        self.browser.sync_page(shown, cx);
    }

    /// Ends the drive from the tab: the page stays as the person's.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub(super) fn stop_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_agent(window, cx);
        self.sync_browser(cx);
        self.focus_right_tab(window, cx);
        cx.notify();
    }

    pub(super) fn render_browser(&self, cx: &mut Context<Self>) -> AnyElement {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
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
                .type_small()
                .text_color(cx.theme().danger)
                .child(note)
        });
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        let page = self.browser.page.clone();
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let page: Option<AnyElement> = None;
        let hint = if WEB {
            "Type an address and press Enter."
        } else {
            UNSUPPORTED
        };
        v_flex()
            .id("browser")
            .ui_container()
            .size_full()
            .gap_2()
            .p_2()
            .when(WEB, |browser| {
                browser.child(Input::new(&self.browser.address))
            })
            .children(note)
            .child(match page {
                Some(page) => div().flex_1().min_h_0().child(page).into_any_element(),
                None => div()
                    .id("browser-hint")
                    .ui_observe_as(Facts {
                        role: Role::Label,
                        ..Facts::button(hint)
                    })
                    .type_small()
                    .child(hint)
                    .into_any_element(),
            })
            .into_any_element()
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
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
            .gap_2()
            .px_2()
            .pt_2()
            .items_center()
            .track_focus(&self.browser.driven_focus)
            .tab_stop(true)
            .child(
                div()
                    .flex_none()
                    .type_small()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("Driven by {thread}")),
            )
            .child(
                div()
                    .id("browser-driven-url")
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .type_small()
                    .text_color(cx.theme().muted_foreground)
                    .child(agent.url.clone()),
            )
            .child(control(
                "browser-stop-agent",
                Facts::button("Stop agent"),
                Button::new("browser-stop-agent-button")
                    .small()
                    .outline()
                    .label("Stop agent")
                    .on_click(cx.listener(|this, _, window, cx| this.stop_agent(window, cx))),
            ));
        let page = self.browser.page.clone();
        v_flex()
            .id("browser")
            .ui_container()
            .size_full()
            .min_h_0()
            .child(row)
            .child(div().flex_1().min_h_0().children(page).into_any_element())
            .into_any_element()
    }

    fn open_address(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.browser.address.read(cx).value().to_string();
        match page_url(&typed) {
            Some(url) => {
                self.browser.note = None;
                self.browser.open(&url, window, cx);
            }
            None => {
                self.browser.note = Some("Type a web address, such as example.com.".into());
            }
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{is_web, page_url};

    #[test]
    fn a_bare_name_opens_over_https() {
        let url = page_url(" example.com ").map(|url| url.to_string());
        assert_eq!(url.as_deref(), Some("https://example.com/"));
    }

    #[test]
    fn a_local_host_opens_over_http() {
        let url = |typed| page_url(typed).map(|url| url.to_string());
        assert_eq!(
            url("localhost:3000").as_deref(),
            Some("http://localhost:3000/")
        );
        assert_eq!(
            url("127.0.0.1:5173/a").as_deref(),
            Some("http://127.0.0.1:5173/a")
        );
        assert_eq!(
            url("localhost.example.com").as_deref(),
            Some("https://localhost.example.com/")
        );
    }

    #[test]
    fn only_web_addresses_open() {
        assert!(page_url("http://example.com/a").is_some());
        assert!(page_url("file:///etc/passwd").is_none());
        assert!(page_url("javascript:alert(1)").is_none());
        assert!(page_url("   ").is_none());
    }

    #[test]
    fn only_web_pages_may_be_navigated_to() {
        assert!(is_web("https://example.com/next"));
        assert!(!is_web("file:///etc/passwd"));
        assert!(!is_web("about:blank"));
    }
}
