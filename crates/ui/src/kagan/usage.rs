//! The provider usage button in the sidebar footer and its popup (decision 249). The window only
//! shows what the `Op` returns; the popup refreshes itself while it is open.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{IconName, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    Anchor, AnyElement, App, Context, Div, ElementId, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, Task, div, px, relative,
};
use kg_core::Workspace;
use kg_domain::{
    Op, OpError, OpResult, UsageAccount, UsageOp, UsageProvider, UsageStatus, UsageView,
    UsageWindow, resets_in,
};

use super::controls::control;
use crate::theme::{SpaceStep, Tone, TypeScale as _, space};
use crate::ui_registry::Facts;

const FRESH_FOR: Duration = Duration::from_secs(120);
const WIDTH: f32 = 320.0;

#[derive(Default)]
pub(crate) struct Usage {
    enabled: bool,
    open: bool,
    view: Option<UsageView>,
    failure: Option<String>,
    loading: bool,
    tab: Option<UsageProvider>,
    fetch: Option<Task<()>>,
    timer: Option<Task<()>>,
}

impl Usage {
    /// Turning the preference off drops the readings, so nothing is shown or kept.
    pub(crate) fn set_enabled(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.enabled == on {
            return;
        }
        if on {
            self.enabled = true;
        } else {
            *self = Self::default();
        }
        cx.notify();
    }

    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn toggle(&mut self, cx: &mut Context<Self>) {
        if self.enabled {
            let open = !self.open;
            self.set_open(open, cx);
        }
    }

    fn set_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.open = open;
        self.timer = None;
        if open {
            self.load(UsageOp::Show, cx);
            self.timer = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(FRESH_FOR).await;
                    if this
                        .update(cx, |usage, cx| usage.load(UsageOp::Show, cx))
                        .is_err()
                    {
                        break;
                    }
                }
            }));
        }
        cx.notify();
    }

    fn load(&mut self, op: UsageOp, cx: &mut Context<Self>) {
        self.loading = true;
        let task = Workspace::dispatch(Op::Usage(op), cx);
        self.fetch = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            _ = this.update(cx, |usage, cx| usage.apply(result, cx));
        }));
        cx.notify();
    }

    fn apply(&mut self, result: Result<OpResult, OpError>, cx: &mut Context<Self>) {
        self.loading = false;
        match result {
            Ok(OpResult::Usage(view)) => {
                self.failure = None;
                self.view = Some(view);
            }
            Ok(_) => {}
            Err(error) => self.failure = Some(error.to_string()),
        }
        if let Some(tab) = self.tab {
            let shown = self
                .view
                .as_ref()
                .is_some_and(|view| view.accounts.iter().any(|a| a.provider == tab));
            if !shown {
                self.tab = None;
            }
        }
        cx.notify();
    }

    fn select(&mut self, tab: Option<UsageProvider>, cx: &mut Context<Self>) {
        self.tab = tab;
        cx.notify();
    }
}

/// The footer button, with its popup. The caller draws nothing when the preference is off.
pub(super) fn control_for(usage: &Entity<Usage>, cx: &mut App) -> AnyElement {
    let open = usage.read(cx).open;
    let owner = usage.clone();
    let content = usage.clone();
    let trigger = Button::new("usage-control")
        .small()
        .ghost()
        .icon(IconName::ChartPie)
        .accessibility_label("Provider usage")
        .tooltip("Provider usage")
        .tab_stop(false);
    control(
        "usage",
        Facts::button("Provider usage"),
        Popover::new("usage-popover")
            .anchor(Anchor::BottomLeft)
            .trigger(trigger)
            .open(open)
            .on_open_change(move |open, _, cx| {
                owner.update(cx, |usage, cx| usage.set_open(*open, cx));
            })
            .content(move |_, _, cx| {
                control(
                    "usage-panel",
                    Facts::button("Provider usage"),
                    panel(&content, cx),
                )
            }),
    )
    .into_any_element()
}

fn panel(usage: &Entity<Usage>, cx: &mut App) -> AnyElement {
    let state = usage.read(cx);
    let view = state.view.clone();
    let loading = state.loading;
    let failure = state.failure.clone();
    let tab = state.tab;
    let gap = px(space(SpaceStep::Sm, cx));
    let mut body = v_flex()
        .w(px(WIDTH))
        .p(px(space(SpaceStep::Md, cx)))
        .gap(gap);
    let Some(view) = view else {
        let text = failure.unwrap_or_else(|| "Loading usage…".to_owned());
        return body
            .child(div().type_small(cx).child(text))
            .child(refresh_row(usage))
            .into_any_element();
    };
    if !view.enabled {
        return body
            .child(div().type_small(cx).child("Provider usage is off."))
            .into_any_element();
    }
    if view.accounts.is_empty() {
        body = body.child(
            div()
                .type_small(cx)
                .text_color(cx.theme().muted_foreground)
                .child("No signed-in providers found."),
        );
        return body.child(refresh_row(usage)).into_any_element();
    }
    body = body.child(tabs(usage, &view.accounts, tab, cx));
    let now = now_secs();
    for account in view
        .accounts
        .iter()
        .filter(|a| tab.is_none_or(|t| t == a.provider))
    {
        body = body.child(account_block(usage, account, now, cx));
    }
    if loading {
        body = body.child(
            div()
                .type_caption(cx)
                .text_color(cx.theme().muted_foreground)
                .child("Refreshing…"),
        );
    }
    body.child(refresh_row(usage)).into_any_element()
}

fn tabs(
    usage: &Entity<Usage>,
    accounts: &[UsageAccount],
    tab: Option<UsageProvider>,
    cx: &mut App,
) -> AnyElement {
    let mut row = h_flex().gap(px(space(SpaceStep::Xs, cx))).flex_wrap();
    let owner = usage.clone();
    row = row.child(
        Button::new("usage-tab-all")
            .small()
            .ghost()
            .label("All")
            .selected(tab.is_none())
            .on_click(move |_, _, cx| owner.update(cx, |usage, cx| usage.select(None, cx))),
    );
    for account in accounts {
        let provider = account.provider;
        let owner = usage.clone();
        row = row.child(
            Button::new(ElementId::Name(
                format!("usage-tab-{}", provider.label()).into(),
            ))
            .small()
            .ghost()
            .label(provider.label())
            .selected(tab == Some(provider))
            .on_click(move |_, _, cx| {
                owner.update(cx, |usage, cx| usage.select(Some(provider), cx));
            }),
        );
    }
    row.into_any_element()
}

fn account_block(
    usage: &Entity<Usage>,
    account: &UsageAccount,
    now: i64,
    cx: &mut App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let mut heading = h_flex().gap(px(space(SpaceStep::Xs, cx))).items_center();
    heading = heading.child(
        div().type_small(cx).child(
            account
                .email
                .clone()
                .unwrap_or_else(|| account.provider.label().to_owned()),
        ),
    );
    if let Some(plan) = &account.plan {
        heading = heading.child(
            div()
                .type_caption(cx)
                .px(px(space(SpaceStep::Xs, cx)))
                .rounded(cx.theme().radius)
                .border_1()
                .border_color(cx.theme().border)
                .child(title_case(plan)),
        );
    }
    let mut block = v_flex()
        .gap(px(space(SpaceStep::Xs, cx)))
        .child(heading)
        .child(
            div()
                .type_caption(cx)
                .text_color(muted)
                .child(account.provider.label()),
        );
    block = match &account.status {
        UsageStatus::Ok => windows(&account.windows, false, now, block, cx),
        UsageStatus::Stale => {
            let block = windows(&account.windows, true, now, block, cx);
            block.child(retry_line(
                usage,
                "Couldn't refresh. Showing last update.",
                cx,
            ))
        }
        UsageStatus::Expired => block.child(
            div()
                .type_caption(cx)
                .text_color(Tone::Failed.color(cx))
                .child("Expired. Sign in again in the provider's app."),
        ),
        UsageStatus::NoLimits => block.child(
            div()
                .type_caption(cx)
                .text_color(muted)
                .child("No subscription limits for this sign-in."),
        ),
        UsageStatus::Error { message } => block.child(retry_line(usage, message, cx)),
    };
    block.into_any_element()
}

fn windows(windows: &[UsageWindow], stale: bool, now: i64, block: Div, cx: &mut App) -> Div {
    windows
        .iter()
        .enumerate()
        .fold(block, |block, (index, window)| {
            block.child(window_row(index, window, stale, now, cx))
        })
}

fn window_row(
    index: usize,
    window: &UsageWindow,
    stale: bool,
    now: i64,
    cx: &mut App,
) -> AnyElement {
    let tone = if window.percent >= 95.0 {
        Tone::Failed
    } else if window.percent >= 80.0 {
        Tone::Needs
    } else {
        Tone::Busy
    };
    let fill = tone.color(cx);
    let muted = cx.theme().muted_foreground;
    let (reset, absolute) = match window.resets_at {
        Some(at) => (format!("{} left", resets_in(at, now)), absolute_reset(at)),
        None => (
            "No reset time".to_owned(),
            "The provider gave no reset time".to_owned(),
        ),
    };
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a percentage is exact in f32"
    )]
    let fraction = (window.percent / 100.0) as f32;
    let track = cx.theme().secondary;
    h_flex()
        .id(ElementId::Name(format!("usage-window-{index}").into()))
        .gap(px(space(SpaceStep::Xs, cx)))
        .items_center()
        .opacity(if stale { 0.6 } else { 1.0 })
        .child(
            div()
                .type_caption(cx)
                .w(px(36.0))
                .child(window.label.clone()),
        )
        .child(
            div().flex_1().h(px(6.0)).rounded(px(3.0)).bg(track).child(
                div()
                    .h_full()
                    .w(relative(fraction))
                    .rounded(px(3.0))
                    .bg(fill),
            ),
        )
        .child(
            div()
                .type_caption(cx)
                .w(px(36.0))
                .text_right()
                .child(format!("{:.0}%", window.percent)),
        )
        .child(
            div()
                .id(ElementId::Name(format!("usage-reset-{index}").into()))
                .type_caption(cx)
                .text_color(muted)
                .w(px(70.0))
                .child(reset)
                .tooltip(move |window, cx| {
                    Tooltip::new(format!("Resets {absolute}")).build(window, cx)
                }),
        )
        .into_any_element()
}

fn retry_line(usage: &Entity<Usage>, text: &str, cx: &mut App) -> AnyElement {
    let owner = usage.clone();
    h_flex()
        .gap(px(space(SpaceStep::Xs, cx)))
        .items_center()
        .child(div().type_caption(cx).flex_1().child(text.to_owned()))
        .child(control(
            "usage-retry",
            Facts::button("Retry"),
            Button::new("usage-retry-control")
                .small()
                .ghost()
                .label("Retry")
                .on_click(move |_, _, cx| {
                    owner.update(cx, |usage, cx| usage.load(UsageOp::Refresh, cx));
                }),
        ))
        .into_any_element()
}

fn refresh_row(usage: &Entity<Usage>) -> AnyElement {
    let owner = usage.clone();
    h_flex()
        .justify_end()
        .child(control(
            "usage-refresh",
            Facts::button("Refresh"),
            Button::new("usage-refresh-control")
                .small()
                .ghost()
                .icon(IconName::RotateCw)
                .label("Refresh")
                .on_click(move |_, _, cx| {
                    owner.update(cx, |usage, cx| usage.load(UsageOp::Refresh, cx));
                }),
        ))
        .into_any_element()
}

fn absolute_reset(at: i64) -> String {
    chrono::DateTime::from_timestamp(at, 0).map_or_else(
        || "at an unknown time".to_owned(),
        |time| time.format("%b %-d, %H:%M UTC").to_string(),
    )
}

fn title_case(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_secs()).ok())
        .unwrap_or(0)
}
