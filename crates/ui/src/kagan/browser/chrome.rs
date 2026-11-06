//! Back, forward, reload, and open-in-the-system-browser for the Browser tab.

use super::Kagan;
use gpui_kit::{AnyElement, Context, IntoElement, div};

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl Kagan {
    pub(super) fn address_row(&self, _cx: &mut Context<Self>) -> AnyElement {
        div().into_any_element()
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
use {
    crate::kagan::controls::control,
    crate::ui_observe::UiObserveExt as _,
    crate::ui_registry::Facts,
    crate::web_page::WebPage,
    gpui_kit::assets::IconName,
    gpui_kit::component::button::{Button, ButtonVariants as _},
    gpui_kit::component::input::Input,
    gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, Sizable as _, h_flex},
    gpui_kit::{
        App, ElementId, Entity, Focusable as _, InteractiveElement as _, ParentElement as _,
        Styled as _,
    },
    std::time::Duration,
};

#[cfg(any(target_os = "macos", target_os = "windows"))]
#[derive(Clone, Copy)]
enum Nav {
    Back,
    Forward,
    Reload,
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl Kagan {
    pub(super) fn address_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let typed = self.browser.address.read(cx).value().to_string();
        let mut field = Input::new(&self.browser.address).w_full();
        if super::secure(&typed) {
            field = field.prefix(Self::secure_mark(cx));
        }
        h_flex()
            .id("browser-chrome")
            .w_full()
            .items_center()
            .gap_1()
            .child(self.browser_nav(cx))
            .child(div().flex_1().min_w_0().child(field))
            .child(self.external_button(cx))
            .into_any_element()
    }

    pub(super) fn browser_nav(&self, cx: &mut Context<Self>) -> AnyElement {
        let idle = self.browser.page.is_none();
        let mut row = h_flex().flex_none().gap_1().items_center();
        for (id, icon, label, nav) in [
            ("browser-back", IconName::ChevronLeft, "Back", Nav::Back),
            (
                "browser-forward",
                IconName::ChevronRight,
                "Forward",
                Nav::Forward,
            ),
            ("browser-reload", IconName::RotateCw, "Reload", Nav::Reload),
        ] {
            row = row.child(Self::nav_button(id, icon, label, nav, idle, cx));
        }
        row.into_any_element()
    }

    pub(super) fn external_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let off = self.external_url(cx).is_none();
        control(
            "browser-external",
            Facts {
                disabled: off,
                hint: Some("Open in external browser".into()),
                ..Facts::button("Open in external browser")
            },
            Button::new("browser-external-button")
                .ghost()
                .small()
                .icon(IconName::ExternalLink)
                .accessibility_label("Open in external browser")
                .tooltip("Open in external browser")
                .disabled(off)
                .on_click(cx.listener(|this, _, _, cx| this.open_in_browser(cx))),
        )
        .into_any_element()
    }

    pub(super) fn secure_mark(cx: &App) -> AnyElement {
        div()
            .id("browser-secure")
            .flex_none()
            .ui_observe_as(Facts {
                role: gpui_kit::Role::Label,
                ..Facts::button("Secure")
            })
            .text_color(cx.theme().success)
            .child(
                Icon::new(IconName::Lock)
                    .small()
                    .text_color(cx.theme().success),
            )
            .into_any_element()
    }

    pub(super) fn follow_address(&mut self, page: Entity<WebPage>, cx: &mut Context<Self>) {
        self.browser.follow = self.browser.follow.wrapping_add(1);
        let generation = self.browser.follow;
        let mut anchor = self.browser.address.read(cx).value().to_string();
        cx.spawn(async move |this, cx| {
            const POLLS: u8 = 20;
            const WAIT: Duration = Duration::from_millis(500);
            for _ in 0..POLLS {
                let step = this.update_in(cx, |this, window, cx| {
                    if this.browser.follow != generation {
                        return None;
                    }
                    let url = page.read(cx).url(cx);
                    if url.is_empty() {
                        return Some(None);
                    }
                    let mut changed = false;
                    if let Some(agent) = &mut this.browser.agent
                        && agent.url != url
                    {
                        agent.url.clone_from(&url);
                        agent.used = std::time::Instant::now();
                        changed = true;
                    }
                    let current = this.browser.address.read(cx).value().to_string();
                    let focused = this
                        .browser
                        .address
                        .read(cx)
                        .focus_handle(cx)
                        .contains_focused(window, cx);
                    let mine =
                        this.browser.agent.as_ref().is_none_or(|agent| {
                            this.shown_thread(cx).as_ref() == Some(&agent.thread)
                        });
                    let wrote = mine && (!focused || current == anchor) && current != url;
                    if wrote {
                        this.browser.address.update(cx, |address, cx| {
                            address.set_value(url.clone(), window, cx);
                        });
                        changed = true;
                    }
                    if changed {
                        cx.notify();
                    }
                    Some(wrote.then_some(url))
                });
                match step {
                    Ok(None) | Err(_) => return,
                    Ok(Some(Some(url))) => anchor = url,
                    Ok(Some(None)) => {}
                }
                cx.background_executor().timer(WAIT).await;
            }
        })
        .detach();
    }

    fn nav_button(
        id: &'static str,
        icon: IconName,
        label: &'static str,
        nav: Nav,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        control(
            id,
            Facts {
                disabled,
                hint: Some(label.into()),
                ..Facts::button(label)
            },
            Button::new(ElementId::Name(format!("button-{id}").into()))
                .ghost()
                .small()
                .icon(icon)
                .accessibility_label(label)
                .tooltip(label)
                .disabled(disabled)
                .on_click(cx.listener(move |this, _, _, cx| this.step(nav, cx))),
        )
        .into_any_element()
    }

    fn step(&mut self, nav: Nav, cx: &mut Context<Self>) {
        let Some(page) = self.browser.page.clone() else {
            return;
        };
        page.read(cx).arm();
        let moved = match nav {
            Nav::Back => page.update(cx, |page, cx| page.back(cx)),
            Nav::Forward => page.update(cx, |page, cx| page.forward(cx)),
            Nav::Reload => page.read(cx).reload(cx),
        };
        if let Err(error) = moved {
            self.browser.note = Some(error);
            cx.notify();
            return;
        }
        self.browser.note = None;
        self.follow_address(page, cx);
    }

    fn open_in_browser(&mut self, cx: &mut Context<Self>) {
        if let Some(url) = self.external_url(cx) {
            cx.open_url(url.as_str());
        }
    }

    pub(super) fn live_url(&self, cx: &App) -> String {
        self.browser
            .page
            .as_ref()
            .map(|page| page.read(cx).url(cx))
            .filter(|url| !url.is_empty())
            .unwrap_or_else(|| match &self.browser.agent {
                Some(agent) => agent.url.clone(),
                None => self.browser.address.read(cx).value().to_string(),
            })
    }

    fn external_url(&self, cx: &App) -> Option<url::Url> {
        super::page_url(&self.live_url(cx))
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(super) async fn evaluate(
    page: &Entity<WebPage>,
    script: &str,
    handle: gpui_kit::AnyWindowHandle,
    wait: Duration,
    cx: &mut gpui_kit::AsyncApp,
) -> Result<String, kg_domain::OpError> {
    use gpui_kit::AppContext as _;
    use kg_domain::OpError;
    let (sender, receiver) = async_channel::bounded::<Option<String>>(1);
    let reply = sender.clone();
    cx.update_window(handle, |_, _, cx| {
        page.read(cx)
            .eval(script, reply, cx)
            .map_err(OpError::Failed)
    })
    .map_err(|_| OpError::NotFound(super::agent::ENDED.into()))??;
    cx.spawn(async move |cx| {
        cx.background_executor().timer(wait).await;
        let _ = sender.try_send(None);
    })
    .detach();
    match receiver.recv().await {
        Ok(Some(text)) if !text.trim().is_empty() => Ok(text),
        Ok(Some(_)) | Err(_) => Err(OpError::Failed(super::snapshot::UNREADABLE.into())),
        Ok(None) => Err(OpError::Failed(
            "the page did not answer within 10 seconds".into(),
        )),
    }
}
