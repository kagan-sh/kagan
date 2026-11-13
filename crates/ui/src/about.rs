use std::path::Path;
use std::time::Duration;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, ClipboardItem, Context, FocusHandle, FontWeight,
    Global, InteractiveElement as _, IntoElement, KeyBinding, ParentElement as _, Render, Role,
    SharedString, Styled as _, Subscription, Task, TitlebarOptions, Window, WindowBounds,
    WindowOptions, actions, div, point, px, size,
};

use crate::kagan::menus::Close;
use crate::theme::TypeScale as _;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

const CONTEXT: &str = "About";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const COMMIT: Option<&str> = option_env!("KG_COMMIT");
const TAGLINE: &str = "A desktop app for coding with agents.";
const FOOTER: &str = "© 2026 Altynbek Orumbayev (MakerX). Apache-2.0 License.";
pub(crate) const REPO: &str = "https://github.com/kagan-sh/kagan";
pub(crate) const BOOK: &str = "https://docs.kagan.sh/";
pub(crate) const LICENSE: &str = "https://github.com/kagan-sh/kagan/blob/main/LICENSE";
pub(crate) const NOTICES_ONLINE: &str =
    "https://github.com/kagan-sh/kagan/blob/main/assets/THIRD-PARTY-NOTICES.txt";
const NOTICES: &str = "THIRD-PARTY-NOTICES.txt";
const COPIED_FOR: Duration = Duration::from_millis(1500);
const ICON: f32 = 96.;
// The corner of kagan-app-icon.svg: rx 229 on a 1024 side.
const ICON_RADIUS: f32 = 229. / 1024.;

actions!(about, [Dismiss]);

pub(crate) fn bindings() -> Vec<KeyBinding> {
    vec![KeyBinding::new("escape", Dismiss, Some(CONTEXT))]
}

struct Shown(AnyWindowHandle);
impl Global for Shown {}

pub(crate) fn show(cx: &mut App) {
    if let Some(&Shown(handle)) = cx.try_global::<Shown>()
        && cx
            .update_window(handle, |_, window, cx| {
                crate::presentation::activate_window(window, cx);
            })
            .is_ok()
    {
        return;
    }
    match gpui_kit::open_window(options(cx), cx, |window, cx| {
        cx.new(|cx| AboutWindow::new(window, cx))
    }) {
        Ok((handle, _)) => cx.set_global(Shown(handle)),
        Err(error) => eprintln!("kg: the About window could not open: {error}"),
    }
}

pub(crate) fn shown(cx: &App) -> Option<AnyWindowHandle> {
    cx.try_global::<Shown>().map(|&Shown(handle)| handle)
}

pub(crate) fn options(cx: &mut App) -> WindowOptions {
    crate::prepare_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(360.), px(336.)),
                cx,
            ))),
            titlebar: Some(TitlebarOptions {
                title: Some("About kagan".into()),
                appears_transparent: cfg!(target_os = "macos"),
                traffic_light_position: Some(point(px(9.), px(9.))),
            }),
            is_resizable: false,
            is_minimizable: false,
            ..WindowOptions::default()
        },
        cx,
    )
}

pub(crate) fn version(commit: Option<&str>) -> String {
    match commit {
        Some(commit) => format!("{VERSION} ({commit})"),
        None => VERSION.to_owned(),
    }
}

// A macOS bundle keeps the notices in Contents/Resources; other builds keep them beside `exe`.
pub(crate) fn notices(exe: &Path) -> SharedString {
    let folder = exe.parent();
    folder
        .and_then(Path::parent)
        .map(|contents| contents.join("Resources").join(NOTICES))
        .into_iter()
        .chain(folder.map(|folder| folder.join(NOTICES)))
        .find(|path| path.is_file())
        .and_then(|path| url::Url::from_file_path(path).ok())
        .map_or(NOTICES_ONLINE.into(), |url| String::from(url).into())
}

struct AboutWindow {
    focus: FocusHandle,
    notices: SharedString,
    copied: Option<Task<()>>,
    was_active: bool,
    _activation: Subscription,
    _appearance: Subscription,
    _theme_paint: Subscription,
}

impl AboutWindow {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let notices =
            std::env::current_exe().map_or_else(|_| NOTICES_ONLINE.into(), |exe| notices(&exe));
        Self {
            focus,
            notices,
            copied: None,
            was_active: window.is_window_active(),
            _activation: cx.observe_window_activation(window, |this, window, _| {
                if window.is_window_active() {
                    this.was_active = true;
                } else if this.was_active {
                    window.remove_window();
                }
            }),
            _appearance: crate::theme::watch_system(window, cx),
            _theme_paint: cx.observe(&crate::theme::ThemeState::global(cx), |_, _, cx| {
                cx.notify();
            }),
        }
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(format!(
            "kagan {}",
            version(COMMIT)
        )));
        self.copied = Some(cx.spawn(async |this, cx| {
            cx.background_executor().timer(COPIED_FOR).await;
            _ = this.update(cx, |this, cx| {
                this.copied = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn link(
        id: &'static str,
        name: &'static str,
        target: SharedString,
        cx: &App,
    ) -> impl IntoElement {
        div()
            .id(id)
            .child(
                Button::new(SharedString::from(format!("{id}-button")))
                    .link()
                    .accessibility_label(name)
                    .child(div().type_small(cx).child(name))
                    .on_click({
                        let target = target.clone();
                        move |_, _, cx| cx.open_url(&target)
                    }),
            )
            .ui_observe_as(Facts {
                role: Role::Link,
                value: Some(target),
                ..Facts::button(name)
            })
    }
}

fn label(text: &'static str) -> Facts {
    Facts {
        role: Role::Label,
        ..Facts::button(text)
    }
}

impl Render for AboutWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let version = format!("Version {}", version(COMMIT));
        let shown = if self.copied.is_some() {
            "Copied".to_owned()
        } else {
            version.clone()
        };
        let separator = || div().text_color(theme.muted_foreground).child("·");
        v_flex()
            .id("about")
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .on_action(|_: &Dismiss, window, _| window.remove_window())
            .on_action(|_: &Close, window, _| window.remove_window())
            .size_full()
            .items_center()
            .pt_8()
            .pb_6()
            .px_6()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(
                div()
                    .id("about-icon")
                    .border_1()
                    .border_color(theme.border)
                    .rounded(px(ICON * ICON_RADIUS))
                    .child(crate::brand::app_icon(ICON))
                    .ui_observe_as(Facts {
                        role: Role::Image,
                        ..Facts::button("kagan")
                    }),
            )
            .child(
                div()
                    .id("about-name")
                    .mt_4()
                    .type_title(cx)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("kagan")
                    .ui_observe_as(label("kagan")),
            )
            .child(
                div()
                    .id("about-version")
                    .mt_1()
                    .child(
                        Button::new("about-version-button")
                            .text()
                            .accessibility_label(version.clone())
                            .text_color(theme.muted_foreground)
                            .child(div().type_caption(cx).child(shown))
                            .on_click(cx.listener(|this, _, _, cx| this.copy(cx))),
                    )
                    .ui_observe_as(Facts {
                        value: self.copied.is_some().then(|| "Copied".into()),
                        ..Facts::button(version)
                    }),
            )
            .child(
                div()
                    .id("about-tagline")
                    .mt_3()
                    .type_small(cx)
                    .text_color(theme.muted_foreground)
                    .child(TAGLINE)
                    .ui_observe_as(label(TAGLINE)),
            )
            .child(
                h_flex()
                    .id("about-links")
                    .mt_5()
                    .gap_2()
                    .type_small(cx)
                    .child(Self::link("about-github", "GitHub", REPO.into(), cx))
                    .child(separator())
                    .child(Self::link("about-book", "Book", BOOK.into(), cx))
                    .child(separator())
                    .child(Self::link("about-license", "License", LICENSE.into(), cx))
                    .child(separator())
                    .child(Self::link(
                        "about-acknowledgements",
                        "Acknowledgements",
                        self.notices.clone(),
                        cx,
                    ))
                    .ui_container(),
            )
            .child(
                div()
                    .id("about-footer")
                    .mt_auto()
                    .type_caption(cx)
                    .text_color(theme.muted_foreground)
                    .child(FOOTER)
                    .ui_observe_as(label(FOOTER)),
            )
            .ui_container()
    }
}
