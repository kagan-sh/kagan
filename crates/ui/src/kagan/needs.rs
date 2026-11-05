use crate::theme::TypeScale as _;
use std::collections::BTreeSet;
use std::time::Duration;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, App, Context, IntoElement, ParentElement as _, Styled as _, Window, div,
};
use kg_core::{Approval, Workspace};
use kg_domain::{InteractionOption, PendingInteractionId};

use super::changes::line;
use super::screen::{Activation, Side};
use super::{Kagan, question};

/// How long the person must stop typing before held questions show. A design value, not a number
/// from the research.
pub(crate) const QUIET: Duration = Duration::from_millis(1500);

/// Cancel is the safe choice, so it takes focus when the card offers it; otherwise the first
/// option does.
fn opening_option(options: &[InteractionOption]) -> Option<&InteractionOption> {
    options
        .iter()
        .find(|option| option.label == "Cancel")
        .or_else(|| options.first())
}

impl Kagan {
    /// Every question that waits for the person, across threads. While the person types, a
    /// question that arrives is held until they pause; one already on screen stays up.
    pub(super) fn render_needs(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.sync_ask_drafts(window, cx);
        let entity = cx.entity();
        let weak = entity.downgrade();
        let approvals: Vec<Approval> = Workspace::global(cx)
            .read(cx)
            .approvals()
            .cloned()
            .collect();
        let mut cards: Vec<AnyElement> = Vec::new();
        let mut held = 0;
        for approval in &approvals {
            if self.holds(&approval.id) {
                held += 1;
            } else {
                cards.push(
                    question::card(
                        question::Question::standalone(approval),
                        None,
                        &entity,
                        &weak,
                        window,
                        cx,
                    )
                    .into_any_element(),
                );
            }
        }
        let questions = self.questions.read(cx).questions.clone();
        let popup = self.popup_ask(cx).map(|asked| asked.id.clone());
        for asked in &questions {
            if self.holds(&asked.id) {
                held += 1;
                continue;
            }
            // The popup owns the fields and answer controls while it is open.
            if popup.as_ref() == Some(&asked.id) {
                continue;
            }
            let thread = self
                .record(&asked.thread_id, cx)
                .map_or_else(String::new, |thread| thread.title.clone());
            let draft = self.ask_drafts.get(&asked.id).cloned();
            cards.push(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .type_small(cx)
                            .text_color(cx.theme().muted_foreground)
                            .child(thread),
                    )
                    .child(question::card(
                        question::Question::thread(asked),
                        draft.as_ref(),
                        &entity,
                        &weak,
                        window,
                        cx,
                    ))
                    .into_any_element(),
            );
        }
        let note = (held > 0).then(|| {
            let text = if held == 1 {
                "1 question waits until you stop typing.".to_owned()
            } else {
                format!("{held} questions wait until you stop typing.")
            };
            line("needs-held", text, cx.theme().muted_foreground, cx)
        });
        v_flex()
            .gap_2()
            .children(note)
            .children(cards)
            .into_any_element()
    }

    /// Whether a question waits: the person is typing, and it was not on screen when they began.
    pub(super) fn holds(&self, id: &PendingInteractionId) -> bool {
        self.typing && !self.on_screen.contains(id)
    }

    pub(super) fn waiting_count(&self, cx: &App) -> usize {
        self.waiting(cx).count()
    }

    /// A person-only question that arrives while the right panels are not drawn opens them over
    /// the center, with Questions shown. Focus does not move, so a person typing keeps the keyboard.
    /// Questions already seen do not open them again, and the overlay closes once nothing waits.
    pub(super) fn announce_questions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let person: BTreeSet<PendingInteractionId> = {
            let workspace = Workspace::global(cx).read(cx);
            self.waiting(cx)
                .filter(|(id, _)| workspace.needs_a_person(id))
                .map(|(id, _)| id.clone())
                .collect()
        };
        self.announced.retain(|id| person.contains(id));
        if !self.has_right_content(cx) {
            self.dismiss_overlay(Side::Right, window, cx);
            return;
        }
        // A question that arrives during review or Settings is not announced yet. Leaving them
        // announces it, so it can open the panels then.
        if self.screen.review().is_some() || self.in_settings() {
            return;
        }
        let opening =
            person.iter().any(|id| !self.announced.contains(id)) && !self.desired_panes(cx).right;
        self.announced.extend(person);
        if opening {
            self.select_right_tab(crate::kagan::right::Tab::Questions, cx);
            if !self.screen.drawn(Side::Right) {
                self.open_overlay(Side::Right, Activation::Quiet, window, cx);
            }
        }
    }

    pub(super) fn overlay_focus(&self, cx: &App) -> String {
        self.waiting(cx)
            .filter(|(id, _)| !self.holds(id))
            .find_map(|(id, options)| {
                opening_option(options).map(|option| format!("answer-focus-{}-{}", id.0, option.id))
            })
            .unwrap_or_else(|| {
                if self.waiting_count(cx) == 0 && self.thread_visible(cx) {
                    "changes-header-focus".into()
                } else {
                    "needs-you-header-focus".into()
                }
            })
    }

    fn waiting<'a>(
        &'a self,
        cx: &'a App,
    ) -> impl Iterator<Item = (&'a PendingInteractionId, &'a [InteractionOption])> {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .map(|approval| (&approval.id, approval.options.as_slice()))
            .chain(
                self.questions
                    .read(cx)
                    .questions
                    .iter()
                    .map(|asked| (&asked.id, asked.options.as_slice())),
            )
    }

    fn waiting_ids(&self, cx: &App) -> BTreeSet<PendingInteractionId> {
        self.waiting(cx).map(|(id, _)| id.clone()).collect()
    }

    /// Starts the pause after a key. Each key restarts it. The first key of a run of typing
    /// records what is on screen, so that only later questions are held.
    pub(super) fn typed(&mut self, cx: &mut Context<Self>) {
        self.cancel_plugin_start();
        if !self.typing {
            self.on_screen = self.waiting_ids(cx);
        }
        self.typing = true;
        self.quiet = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(QUIET).await;
            _ = this.update(cx, |this, cx| {
                this.typing = false;
                this.on_screen.clear();
                cx.notify();
            });
        });
        cx.notify();
    }
}
