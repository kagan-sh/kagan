use crate::theme::TypeScale as _;
use std::collections::BTreeMap;

use gpui_kit::assets::IconName;
use gpui_kit::component::input::Input;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Action, AnyElement, App, Context, ElementId, Focusable as _, InteractiveElement as _,
    IntoElement, ParentElement as _, StatefulInteractiveElement as _, Styled as _, Window, div,
};
use kg_domain::{DiffFile, DiffStatus};

use crate::kagan::controls::Selected;
use crate::theme;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

use super::{
    CONTEXT, Down, End, FILE_INSET, FOLDER_INSET, Folder, Home, Left, Open, Right, TreeKey,
    TreeRow, Up, compact_folder, folder_tree, ordered_files, tree_order,
};
use crate::kagan::Kagan;

impl Kagan {
    fn tree_key(
        &mut self,
        path: &str,
        rows: &[TreeRow],
        key: TreeKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = rows.iter().position(|row| row.path == path) else {
            return;
        };
        let row = &rows[index];
        let target = match key {
            TreeKey::Up => Some(index.saturating_sub(1)),
            TreeKey::Down => Some((index + 1).min(rows.len() - 1)),
            TreeKey::Home => Some(0),
            TreeKey::End => Some(rows.len() - 1),
            TreeKey::Left
                if row.folder
                    && self
                        .screen
                        .review()
                        .is_some_and(|review| !review.collapsed.contains(path)) =>
            {
                if let Some(review) = self.screen.review_mut() {
                    review.collapsed.insert(path.to_owned());
                }
                cx.notify();
                None
            }
            TreeKey::Left => rows[..index].iter().rposition(|candidate| {
                candidate.folder
                    && path
                        .strip_prefix(&candidate.path)
                        .is_some_and(|suffix| suffix.starts_with('/'))
            }),
            TreeKey::Right
                if row.folder
                    && self
                        .screen
                        .review()
                        .is_some_and(|review| review.collapsed.contains(path)) =>
            {
                if let Some(review) = self.screen.review_mut() {
                    review.collapsed.remove(path);
                }
                cx.notify();
                None
            }
            TreeKey::Right if row.folder && index + 1 < rows.len() => Some(index + 1),
            TreeKey::Open if row.folder => {
                if let Some(review) = self.screen.review_mut()
                    && !review.collapsed.remove(path)
                {
                    review.collapsed.insert(path.to_owned());
                }
                cx.notify();
                None
            }
            TreeKey::Open => {
                self.select_review_file(path, cx);
                None
            }
            TreeKey::Right => None,
        };
        if let Some(target) = target {
            if let Some(review) = self.screen.review_mut() {
                review.tree_focus = Some(rows[target].path.clone());
            }
            window.focus(&rows[target].focus, cx);
            cx.notify();
        }
    }
    pub(in crate::kagan) fn review_files(&self, visible: bool, cx: &Context<Self>) -> Vec<String> {
        let Some(review) = self.screen.review() else {
            return Vec::new();
        };
        let query = review.filter.read(cx).value().to_lowercase();
        let files: Vec<_> = self
            .changes
            .files
            .iter()
            .filter(|file| !visible || file.path.to_lowercase().contains(&query))
            .collect();
        if !review.tree {
            return files.iter().map(|file| file.path.clone()).collect();
        }
        let root = folder_tree(&files);
        let mut order = Vec::new();
        let expanded = std::collections::BTreeSet::new();
        ordered_files(
            &root,
            "",
            if visible {
                &review.collapsed
            } else {
                &expanded
            },
            &mut order,
        );
        order
            .into_iter()
            .filter_map(|(path, folder)| (!folder).then_some(path))
            .collect()
    }

    pub(in crate::kagan) fn review_tree(
        &self,
        narrow: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(review) = self.screen.review() else {
            return div().into_any_element();
        };
        let focus = review.filter.focus_handle(cx);
        let query = review.filter.read(cx).value().to_lowercase();
        let files: Vec<&DiffFile> = self
            .changes
            .files
            .iter()
            .filter(|file| file.path.to_lowercase().contains(&query))
            .collect();
        let root = folder_tree(&files);
        let order = tree_order(&root, &files, review, window, cx);
        let lookup: BTreeMap<_, _> = order.iter().map(|row| (row.path.clone(), row)).collect();
        let rows = if review.tree {
            self.tree_rows(&root, "", 0, &lookup, window, cx)
        } else {
            files
                .iter()
                .map(|file| self.tree_file(file, &file.path, 0, &lookup, window, cx))
                .collect()
        };
        v_flex()
            .h_full()
            .w_56()
            .flex_shrink_0()
            .min_h_0()
            .border_l_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .h_9()
                    .flex_none()
                    .px_2()
                    .gap_2()
                    .items_center()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .when(narrow, |bar| {
                        bar.child(Self::review_pane_toggle(review, cx))
                    })
                    .child(
                        div()
                            .id("review-filter")
                            .flex_1()
                            .min_w_0()
                            .ui_observe_as(Facts::input("Filter files", &review.filter, cx))
                            .on_mouse_down(gpui_kit::MouseButton::Left, move |_, window, cx| {
                                window.focus(&focus, cx);
                            })
                            .child(
                                Input::new(&review.filter)
                                    .text_size(gpui_kit::px(crate::theme::type_body(cx)))
                                    .small(),
                            ),
                    )
                    .child(
                        div().flex_none().child(
                            crate::kagan::controls::segments(cx)
                                .child(Self::review_tree_toggle(review.tree, true, cx))
                                .child(Self::review_tree_toggle(review.tree, false, cx)),
                        ),
                    ),
            )
            .child(Self::tree_list(rows, &order, &query, files.is_empty(), cx))
            .into_any_element()
    }

    fn tree_list(
        rows: Vec<AnyElement>,
        order: &std::rc::Rc<Vec<TreeRow>>,
        query: &str,
        empty: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .key_context("ReviewNav")
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .key_context(CONTEXT)
                    .flex_1()
                    .min_h_0()
                    .px_2()
                    .py_1()
                    .ui_container()
                    .overflow_y_scrollbar()
                    .id("review-tree-list")
                    .on_action(Self::on_tree::<Up>(order, TreeKey::Up, cx))
                    .on_action(Self::on_tree::<Down>(order, TreeKey::Down, cx))
                    .on_action(Self::on_tree::<Left>(order, TreeKey::Left, cx))
                    .on_action(Self::on_tree::<Right>(order, TreeKey::Right, cx))
                    .on_action(Self::on_tree::<Home>(order, TreeKey::Home, cx))
                    .on_action(Self::on_tree::<End>(order, TreeKey::End, cx))
                    .on_action(Self::on_tree::<Open>(order, TreeKey::Open, cx))
                    .children(rows)
                    .when(empty, |list| {
                        list.child(
                            div()
                                .p_2()
                                .type_body(cx)
                                .text_color(cx.theme().muted_foreground)
                                .child(format!("No files match “{query}”.")),
                        )
                    }),
            )
            .into_any_element()
    }

    fn on_tree<A: Action>(
        order: &std::rc::Rc<Vec<TreeRow>>,
        key: TreeKey,
        cx: &Context<Self>,
    ) -> impl Fn(&A, &mut Window, &mut App) + 'static {
        let order = order.clone();
        cx.listener(move |this, _: &A, window, cx| {
            if let Some(row) = order.iter().find(|row| row.focus.is_focused(window)) {
                this.tree_key(&row.path, &order, key, window, cx);
            }
        })
    }

    fn review_tree_toggle(tree: bool, target: bool, cx: &mut Context<Self>) -> AnyElement {
        let label = if target { "Tree" } else { "Path" };
        let id = if target { "review-tree" } else { "review-path" };
        let chosen = tree == target;
        crate::kagan::controls::control(
            id,
            Facts {
                selected: Some(chosen),
                ..Facts::button(label)
            },
            crate::kagan::controls::segment(
                ElementId::Name(format!("button-{id}").into()),
                if chosen { Selected::Yes } else { Selected::No },
                cx,
            )
            .icon(if target {
                gpui_kit::assets::IconName::ListTree
            } else {
                gpui_kit::assets::IconName::List
            })
            .tooltip(label)
            .on_click(cx.listener(move |this, _, window, cx| {
                if let Some(review) = this.screen.review_mut() {
                    review.tree = target;
                }
                this.focus_review(window, cx);
                cx.notify();
            })),
        )
        .into_any_element()
    }

    fn tree_rows(
        &self,
        folder: &Folder<'_>,
        parent: &str,
        depth: u16,
        lookup: &BTreeMap<String, &TreeRow>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut rows = Vec::new();
        for (name, child) in &folder.folders {
            let (name, path, child) = compact_folder(name, child, parent);
            let collapsed = self
                .screen
                .review()
                .is_some_and(|review| review.collapsed.contains(&path));
            let key = path.clone();
            let Some(item) = lookup.get(&path) else {
                continue;
            };
            let focus = item.focus.clone();
            let focused = focus.is_focused(window);
            let click_focus = focus.clone();
            rows.push(
                h_flex()
                    .id(ElementId::Name(format!("review-folder-{path}").into()))
                    .ui_observe_as(Facts {
                        focus: Some(focus.clone()),
                        ..Facts::button(path.clone())
                    })
                    .track_focus(&focus)
                    .tab_stop(item.tab)
                    .when(focused, |row| row.border_1().border_color(cx.theme().ring))
                    .pl(gpui_kit::px(f32::from(depth) * 14. + FOLDER_INSET))
                    .pr_2()
                    .h_7()
                    .gap_1()
                    .rounded(cx.theme().radius)
                    .type_body(cx)
                    .hover(|row| row.bg(cx.theme().list_hover))
                    .child(
                        Icon::new(if collapsed {
                            IconName::ChevronRight
                        } else {
                            IconName::ChevronDown
                        })
                        .xsmall()
                        .text_color(cx.theme().muted_foreground),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_ellipsis()
                            .text_color(cx.theme().muted_foreground)
                            .child(name),
                    )
                    .child(
                        div()
                            .text_color(cx.theme().muted_foreground)
                            .child(child.count.to_string()),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(review) = this.screen.review_mut()
                            && !review.collapsed.remove(&key)
                        {
                            review.collapsed.insert(key.clone());
                        }
                        if let Some(review) = this.screen.review_mut() {
                            review.tree_focus = Some(key.clone());
                        }
                        window.focus(&click_focus, cx);
                        cx.notify();
                    }))
                    .into_any_element(),
            );
            if !collapsed {
                rows.extend(self.tree_rows(child, &path, depth + 1, lookup, window, cx));
            }
        }
        for file in &folder.files {
            let name = file.path.rsplit('/').next().unwrap_or(&file.path);
            rows.push(self.tree_file(file, name, depth, lookup, window, cx));
        }
        rows
    }

    fn tree_file(
        &self,
        file: &DiffFile,
        name: &str,
        depth: u16,
        lookup: &BTreeMap<String, &TreeRow>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let path = file.path.clone();
        let Some(item) = lookup.get(&path) else {
            return div().into_any_element();
        };
        let focus = item.focus.clone();
        let focused = focus.is_focused(window);
        let click_focus = focus.clone();
        let selected = self.screen.review().map(|review| &review.path) == Some(&path);
        let kind = crate::kagan::changes::kind(file.status);
        let viewed = self.is_viewed(&file.path, cx);
        let (base, folder) = name
            .rsplit_once('/')
            .map_or((name, ""), |(folder, base)| (base, folder));
        h_flex()
            .id(ElementId::Name(format!("review-file-{path}").into()))
            .ui_observe_as(Facts {
                focus: Some(focus.clone()),
                selected: Some(selected),
                ..Facts::button(path.clone())
            })
            .track_focus(&focus)
            .tab_stop(item.tab)
            .when(focused, |row| row.border_1().border_color(cx.theme().ring))
            .pl(gpui_kit::px(f32::from(depth) * 14. + FILE_INSET))
            .pr_2()
            .h_7()
            .gap_1()
            .rounded(cx.theme().radius)
            .type_body(cx)
            .when(selected, |row| row.bg(cx.theme().secondary))
            .hover(|row| row.bg(cx.theme().list_hover))
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .flex_shrink_0()
                            .when(viewed && !selected, |name| {
                                name.text_color(cx.theme().muted_foreground)
                            })
                            .when(file.status == DiffStatus::Deleted, |name| {
                                name.line_through().text_color(cx.theme().muted_foreground)
                            })
                            .child(base.to_owned()),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .text_ellipsis()
                            .text_color(cx.theme().muted_foreground)
                            .child(if folder.is_empty() {
                                String::new()
                            } else {
                                format!("  {folder}")
                            }),
                    ),
            )
            .when(viewed, |row| row.child("✓"))
            .when(file.added > 0, |row| {
                row.child(
                    div()
                        .type_caption(cx)
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_color(theme::added(cx))
                        .child(format!("+{}", file.added)),
                )
            })
            .when(file.removed > 0, |row| {
                row.child(
                    div()
                        .type_caption(cx)
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_color(cx.theme().danger)
                        .child(format!("−{}", file.removed)),
                )
            })
            .child(
                div()
                    .w_3()
                    .type_caption(cx)
                    .font_family(cx.theme().mono_font_family.clone())
                    .font_weight(gpui_kit::FontWeight::MEDIUM)
                    .text_color(cx.theme().muted_foreground)
                    .child(kind),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                if let Some(review) = this.screen.review_mut() {
                    review.tree_focus = Some(path.clone());
                }
                this.select_review_file(&path, cx);
                window.focus(&click_focus, cx);
            }))
            .into_any_element()
    }
}
