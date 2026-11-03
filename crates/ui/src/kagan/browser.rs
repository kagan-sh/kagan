//! The Browser tab of the right sidebar: an address field and one web page, drawn by the native
//! web view. The page is macOS only for now (`WKWebView`). Other systems show a note instead.

use crate::theme::TypeScale as _;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, Focusable as _, InteractiveElement as _,
    IntoElement, ParentElement as _, Role, Styled as _, Subscription, Window, div,
};

use super::Kagan;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

#[cfg(target_os = "macos")]
use gpui_webview::WebView;

pub(super) struct Browser {
    address: Entity<InputState>,
    #[cfg(target_os = "macos")]
    page: Option<Entity<WebView>>,
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
            #[cfg(target_os = "macos")]
            page: None,
            note: None,
            _enter: enter,
        }
    }

    #[cfg(target_os = "macos")]
    fn open(&mut self, url: &url::Url, window: &mut Window, cx: &mut Context<Kagan>) {
        if let Some(page) = &self.page {
            page.update(cx, |page, _| page.load_url(url.as_str()));
            return;
        }
        // Links and redirects inside the page are held to the same rule as the address field.
        // Private: cookies and logins end with the page, and nothing lands outside kagan's data.
        let builder = crate::web::builder(url.as_str(), |target| is_web(&target), None);
        match WebView::build(builder, window, cx) {
            Ok(view) => self.page = Some(cx.new(|_| view)),
            Err(error) => self.note = Some(format!("The page could not open: {error}")),
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn open(&mut self, _: &url::Url, _: &mut Window, _: &mut Context<Kagan>) {
        self.note = Some(UNSUPPORTED.into());
    }

    #[cfg(target_os = "macos")]
    fn sync_page(&self, shown: bool, cx: &mut App) {
        let Some(page) = &self.page else { return };
        if page.read(cx).visible() != shown {
            page.update(cx, |page, _| if shown { page.show() } else { page.hide() });
        }
    }

    /// Drops the page and its address, so nothing on it keeps running once its tab is gone.
    pub(super) fn close(&mut self, window: &mut Window, cx: &mut App) {
        #[cfg(target_os = "macos")]
        {
            self.page = None;
        }
        self.note = None;
        self.address.update(cx, |address, cx| {
            address.set_value(String::new(), window, cx);
        });
    }

    #[cfg(all(test, target_os = "macos"))]
    pub(super) fn address(&self, cx: &App) -> String {
        self.address.read(cx).value().to_string()
    }

    pub(super) fn holds_focus(&self, window: &Window, cx: &App) -> bool {
        self.address.focus_handle(cx).contains_focused(window, cx)
    }

    pub(super) fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.address.focus_handle(cx), cx);
    }
}

const WEB: bool = cfg!(target_os = "macos");
const UNSUPPORTED: &str = "The browser works on macOS only for now.";

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
    #[cfg(target_os = "macos")]
    pub(super) fn browser_drawn(&self, cx: &App) -> bool {
        self.right_drawn()
            && super::right::selected_tab(self, cx).as_deref() == Some(super::tabs::BROWSER)
    }

    /// Hides or shows the native page to match whether the Browser tab is on screen.
    #[cfg(target_os = "macos")]
    pub(super) fn sync_browser(&self, cx: &mut App) {
        let shown = self.browser_drawn(cx);
        self.browser.sync_page(shown, cx);
    }

    pub(super) fn render_browser(&self, cx: &App) -> AnyElement {
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
        #[cfg(target_os = "macos")]
        let page = self.browser.page.clone();
        #[cfg(not(target_os = "macos"))]
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
