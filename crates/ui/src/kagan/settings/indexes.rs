use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, ClickEvent, Context, ElementId, Entity,
    InteractiveElement as _, IntoElement as _, ParentElement as _, StatefulInteractiveElement as _,
    Styled as _, Window, div,
};
use kg_domain::{IndexOp, OpResult, PluginOp};
use kg_plugins::PluginHost;

use super::Kagan;
use super::row::{Group, Row, group as row_group, row};
use crate::kagan::controls::control;
use crate::kagan::plugin_actions::{AddIndex, SearchIndexes};
use crate::theme::TypeScale as _;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

pub(super) fn group(kagan: &Kagan, cx: &mut Context<Kagan>) -> Group {
    let saved = PluginHost::try_global(cx).map_or(Ok(Vec::new()), |host| host.read(cx).indexes());
    let (locations, broken) = match saved {
        Ok(locations) => (locations, None),
        Err(error) => (Vec::new(), Some(error.to_string())),
    };
    let mut rows: Vec<Row> = locations
        .iter()
        .enumerate()
        .map(|(at, location)| {
            let removed = location.clone();
            let remove = button(
                &format!("plugin-index-remove-{at}"),
                "Remove",
                false,
                cx.listener(move |this, _, _, cx| {
                    let op = PluginOp::Index {
                        op: IndexOp::Remove {
                            location: removed.clone(),
                        },
                    };
                    this.plugin_op(
                        op,
                        "remove the index",
                        |this, _, _| this.plugins.found = None,
                        cx,
                    );
                }),
            );
            let note = if at == 0 { "Searched first." } else { "" };
            row(&format!("index-{at}"), location, note, remove)
        })
        .collect();
    if let Some(error) = broken {
        rows.push(row(
            "index-broken",
            "The list of indexes cannot be read",
            &format!("Fix or delete it: {error}"),
            div().into_any_element(),
        ));
    }
    rows.push(add_row(kagan, cx));
    if !locations.is_empty() {
        let search = button(
            "plugin-search-run",
            "Search",
            false,
            cx.listener(|this, _, _, cx| this.search_indexes(cx)),
        );
        let search = field(
            "plugin-search",
            "Find a plugin",
            &kagan.plugins.query,
            search,
            cx,
        )
        .key_context("PluginSearch")
        .on_action(cx.listener(|this, _: &SearchIndexes, _, cx| this.search_indexes(cx)));
        rows.push(Row {
            wide: true,
            ..row(
                "index-search",
                "Find a plugin",
                "Search by name or description. Install asks you to approve the plugin's powers.",
                search.into_any_element(),
            )
        });
    }
    rows.extend(found_rows(kagan, cx));
    row_group("indexes", "Indexes", rows)
}

fn found_rows(kagan: &Kagan, cx: &mut Context<Kagan>) -> Vec<Row> {
    let Some((listings, warnings)) = &kagan.plugins.found else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    let installing =
        PluginHost::try_global(cx).is_some_and(|host| host.read(cx).installing().is_some());
    for listing in listings {
        let name = format!("{}/{}", listing.index, listing.name);
        let install = button(
            &format!("plugin-listing-install-{name}"),
            "Install",
            installing,
            cx.listener(move |this, _, _, cx| {
                let op = PluginOp::Install {
                    source: name.clone(),
                    plugin: None,
                    thread_id: None,
                };
                this.plugin_op(op, "install the plugin", |_, _, _| {}, cx);
            }),
        );
        let note = format!("{} · {}", listing.description, listing.index);
        rows.push(row(
            &format!("listing-{}/{}", listing.index, listing.name),
            &listing.name,
            &note,
            install,
        ));
    }
    if listings.is_empty() {
        rows.push(row(
            "listing-none",
            "No plugin matches",
            "",
            div().into_any_element(),
        ));
    }
    for (at, warning) in warnings.iter().enumerate() {
        rows.push(row(
            &format!("index-warning-{at}"),
            "Index not read",
            warning,
            div().into_any_element(),
        ));
    }
    rows
}

fn button(
    id: &str,
    label: &'static str,
    disabled: bool,
    click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    control(
        id.to_owned(),
        Facts::button(label),
        Button::new(ElementId::Name(format!("button-{id}").into()))
            .small()
            .outline()
            .label(label)
            .disabled(disabled)
            .on_click(click),
    )
    .into_any_element()
}

fn field(
    id: &'static str,
    name: &'static str,
    state: &Entity<InputState>,
    run: AnyElement,
    cx: &mut Context<Kagan>,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    let input = div()
        .id(id)
        .ui_observe_as(Facts::input(name, state, cx))
        .flex_1()
        .min_w_0()
        .child(
            Input::new(state)
                .text_size(gpui_kit::px(crate::theme::type_body(cx)))
                .small(),
        );
    div()
        .id(format!("{id}-form"))
        .child(h_flex().gap_2().child(input).child(run))
}

impl Kagan {
    fn add_index(&mut self, cx: &mut Context<Self>) {
        let location = self.plugins.index.read(cx).value().trim().to_owned();
        if location.is_empty() {
            return;
        }
        let op = PluginOp::Index {
            op: IndexOp::Add { location },
        };
        self.plugins.index_error = None;
        self.run_plugin_op(
            op,
            |this, result, cx| match result {
                Ok(_) => {
                    this.plugins.found = None;
                    let window = this.window;
                    let input = this.plugins.index.clone();
                    cx.spawn(async move |_, cx| {
                        _ = cx.update_window(window, |_, window, cx| {
                            input
                                .update(cx, |state, cx| state.set_value(String::new(), window, cx));
                        });
                    })
                    .detach();
                }
                Err(error) => this.plugins.index_error = Some(error.to_string()),
            },
            cx,
        );
    }

    fn search_indexes(&mut self, cx: &mut Context<Self>) {
        let text = self.plugins.query.read(cx).value().trim().to_owned();
        self.plugin_op(
            PluginOp::Search { text },
            "search the indexes",
            |this, result, _| {
                if let Ok(OpResult::Listings { listings, warnings }) = result {
                    this.plugins.found = Some((listings.clone(), warnings.clone()));
                }
            },
            cx,
        );
    }
}

fn add_row(kagan: &Kagan, cx: &mut Context<Kagan>) -> Row {
    let add = button(
        "plugin-index-add",
        "Add",
        false,
        cx.listener(|this, _, _, cx| this.add_index(cx)),
    );
    let add = field(
        "plugin-index",
        "Index address",
        &kagan.plugins.index,
        add,
        cx,
    )
    .key_context("PluginIndex")
    .on_action(cx.listener(|this, _: &AddIndex, _, cx| this.add_index(cx)));
    let add_row = row(
        "index-add",
        "Add an index",
        "A git address with a ref, git:<url>@<ref>, or a folder or file on this computer. Private repositories use your own git sign-in.",
        add.into_any_element(),
    );
    Row {
        wide: true,
        note_content: kagan.plugins.index_error.as_ref().map(|error| {
            div()
                .id("plugin-index-error")
                .ui_container()
                .aria_label(error.clone())
                .type_small(cx)
                .text_color(cx.theme().danger)
                .child(error.clone())
                .into_any_element()
        }),
        ..add_row
    }
}
