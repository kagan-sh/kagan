use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::{
    App, AppContext as _, Bounds, Context, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Role, SharedString, Styled as _, Subscription, TitlebarOptions,
    Window, WindowBounds, WindowOptions, div, px, size,
};

use crate::theme::TypeScale as _;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

pub(crate) fn show(message: SharedString, cx: &mut App) -> gpui_kit::Result<()> {
    gpui_kit::open_window(options(cx), cx, |_, cx| {
        cx.new(|cx| StartFailed {
            message,
            _quit: cx.on_release(|_, cx| cx.quit()),
        })
    })?;
    crate::presentation::activate(cx);
    Ok(())
}

pub(crate) fn options(cx: &mut App) -> WindowOptions {
    crate::prepare_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(440.), px(240.)),
                cx,
            ))),
            titlebar: Some(TitlebarOptions {
                title: Some("kagan could not start".into()),
                ..TitlebarOptions::default()
            }),
            is_resizable: false,
            is_minimizable: false,
            ..WindowOptions::default()
        },
        cx,
    )
}

struct StartFailed {
    message: SharedString,
    _quit: Subscription,
}

impl Render for StartFailed {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        v_flex()
            .id("start-failed")
            .size_full()
            .gap_4()
            .p_6()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(
                div()
                    .id("start-failed-message")
                    .type_small(cx)
                    .child(self.message.clone())
                    .ui_observe_as(Facts {
                        role: Role::Label,
                        ..Facts::button(self.message.clone())
                    }),
            )
            .child(
                div()
                    .id("start-failed-quit")
                    .mt_auto()
                    .child(
                        Button::new("start-failed-quit-button")
                            .primary()
                            .label("Quit")
                            .on_click(|_, _, cx| cx.quit()),
                    )
                    .ui_observe_as(Facts::button("Quit")),
            )
            .ui_container()
    }
}
