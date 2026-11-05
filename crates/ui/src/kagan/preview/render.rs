use std::sync::Arc;

use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::table::{Table, TableBody, TableCell, TableHead, TableHeader, TableRow};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme as _, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, ClipboardItem, Context, Image, ImageSource, InteractiveElement as _, IntoElement,
    ObjectFit, ParentElement as _, Role, SharedString, StatefulInteractiveElement as _,
    Styled as _, StyledImage as _, div, img, px,
};
use kg_domain::{PreviewCut, PreviewImageFormat, PreviewKind};

use super::super::Kagan;
use super::Preview;
use crate::kagan::controls::control;
use crate::theme::{TypeScale as _, type_mono};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

impl Preview {
    pub(in crate::kagan) fn render(&self, cx: &mut Context<Kagan>) -> AnyElement {
        let (directory, name) = self
            .path
            .rsplit_once(['/', '\\'])
            .map_or(("", self.path.as_str()), |(directory, name)| {
                (directory, name)
            });
        let size = self
            .file
            .as_ref()
            .map_or_else(String::new, |file| kg_core::human_size(file.bytes));
        let path = self.path.clone();
        v_flex()
            .id("preview")
            .ui_container()
            .size_full()
            .min_h_0()
            .track_focus(&self.focus)
            .child(
                h_flex()
                    .id("preview-header")
                    .ui_container()
                    .flex_none()
                    .items_center()
                    .gap_2()
                    .h_9()
                    .px_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        h_flex()
                            .id("preview-name")
                            .flex_1()
                            .min_w_0()
                            .whitespace_nowrap()
                            .type_small(cx)
                            .when(!directory.is_empty(), |title| {
                                title.child(
                                    div()
                                        .min_w_0()
                                        .text_ellipsis_start()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(format!("{directory}/")),
                                )
                            })
                            .child(
                                div()
                                    .flex_none()
                                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                    .child(name.to_owned()),
                            )
                            .ui_container(),
                    )
                    .child(
                        div()
                            .id("preview-size")
                            .ui_observe_as(Facts {
                                role: Role::Label,
                                value: Some(size.clone().into()),
                                ..Facts::button("File size")
                            })
                            .flex_none()
                            .type_small(cx)
                            .text_color(cx.theme().muted_foreground)
                            .child(size),
                    )
                    .child(control(
                        "preview-copy",
                        Facts::button("Copy path"),
                        gpui_kit::component::button::Button::new("preview-copy-button")
                            .xsmall()
                            .ghost()
                            .icon(IconName::Copy)
                            .accessibility_label("Copy path")
                            .tooltip("Copy path")
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(path.clone()));
                            })),
                    )),
            )
            .child(self.body(cx))
            .into_any_element()
    }

    fn body(&self, cx: &mut Context<Kagan>) -> AnyElement {
        if let Some(notice) = &self.notice {
            return sentence("preview-notice", notice.clone(), cx);
        }
        let Some(file) = self.file.as_ref() else {
            let text = if self.loading {
                "Reading…"
            } else {
                "No file."
            };
            return sentence("preview-loading", text.into(), cx);
        };
        match &file.kind {
            PreviewKind::Unavailable { reason } => {
                sentence("preview-notice", reason.clone().into(), cx)
            }
            PreviewKind::Text { text, cut } => plain_text(text, cut.as_ref(), self, cx),
            PreviewKind::Markdown { text, cut } => markdown(text, cut.as_ref(), cx),
            PreviewKind::Csv {
                rows,
                rows_total,
                columns_total,
                truncated,
            } => csv(rows, *rows_total, *columns_total, *truncated, cx),
            PreviewKind::Image {
                format,
                width,
                height,
            } => image_body(self.image.clone(), *format, *width, *height, file.bytes, cx),
        }
    }
}

fn cut_line(cut: &PreviewCut) -> String {
    format!("Showing the first {} of {} lines.", cut.shown, cut.total)
}

fn sentence(id: &'static str, text: SharedString, cx: &mut Context<Kagan>) -> AnyElement {
    div()
        .id(id)
        .ui_observe_as(Facts {
            role: Role::Label,
            value: Some(text.clone()),
            ..Facts::button("Preview notice")
        })
        .type_small(cx)
        .text_color(cx.theme().muted_foreground)
        .p_3()
        .child(text)
        .into_any_element()
}

fn plain_text(
    text: &str,
    cut: Option<&PreviewCut>,
    preview: &Preview,
    cx: &mut Context<Kagan>,
) -> AnyElement {
    v_flex()
        .id("preview-text")
        .flex_1()
        .min_h_0()
        .child(
            div()
                .id("preview-text-rows")
                .ui_container()
                .size_full()
                .overflow_scroll()
                .track_scroll(&preview.scroll)
                .child(
                    v_flex()
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_size(px(type_mono(cx)))
                        .whitespace_nowrap()
                        .p_2()
                        .children(
                            text.lines()
                                .map(|line| div().child(line.to_owned()))
                                .collect::<Vec<_>>(),
                        ),
                ),
        )
        .children(cut.map(|cut| sentence("preview-cut", cut_line(cut).into(), cx)))
        .into_any_element()
}

fn markdown(text: &str, cut: Option<&PreviewCut>, cx: &mut Context<Kagan>) -> AnyElement {
    v_flex()
        .id("preview-markdown")
        .flex_1()
        .min_h_0()
        .child(
            div()
                .id("preview-markdown-rows")
                .ui_container()
                .size_full()
                .overflow_y_scroll()
                .p_3()
                .child(
                    TextView::markdown("preview-markdown-text", text.to_owned())
                        .selectable(true)
                        .scrollable(false)
                        .image_source(|_| ImageSource::Image(Arc::new(Image::empty())))
                        .on_link_click(|url, _, _, cx| {
                            if is_web(url) {
                                cx.open_url(url);
                            }
                        }),
                ),
        )
        .children(cut.map(|cut| sentence("preview-cut", cut_line(cut).into(), cx)))
        .into_any_element()
}

fn is_web(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
}

fn csv(
    rows: &[Vec<String>],
    rows_total: usize,
    columns_total: usize,
    truncated: bool,
    cx: &mut Context<Kagan>,
) -> AnyElement {
    let mut table = Table::new().small().accessibility_label("CSV preview");
    if let Some(header) = rows.first() {
        table = table.child(
            TableHeader::new().child(
                TableRow::new().children(
                    header
                        .iter()
                        .map(|cell| TableHead::new().child(cell.clone())),
                ),
            ),
        );
    }
    let body = rows.iter().skip(1).fold(TableBody::new(), |body, row| {
        body.child(
            TableRow::new().children(
                row.iter()
                    .map(|cell| TableCell::new().child(cell.clone()))
                    .collect::<Vec<_>>(),
            ),
        )
    });
    let note = truncated.then(|| {
        format!(
            "Showing the first {} of {} rows, and the first {} of {} columns.",
            rows.len(),
            rows_total,
            rows.first().map_or(0, Vec::len),
            columns_total
        )
    });
    v_flex()
        .id("preview-csv")
        .flex_1()
        .min_h_0()
        .gap_2()
        .child(
            div()
                .id("preview-csv-table")
                .ui_container()
                .w_full()
                .min_w_0()
                .p_2()
                .overflow_x_scroll()
                .child(table.child(body)),
        )
        .children(note.map(|note| sentence("preview-cut", note.into(), cx)))
        .into_any_element()
}

fn image_body(
    image: Option<Arc<Image>>,
    format: PreviewImageFormat,
    width: u32,
    height: u32,
    bytes: u64,
    cx: &mut Context<Kagan>,
) -> AnyElement {
    let caption = format!(
        "{width}×{height} · {} {}",
        format.name(),
        kg_core::human_size(bytes)
    );
    let picture = image.map_or_else(
        || sentence("preview-image-loading", "Reading the image…".into(), cx),
        |image| {
            div()
                .id("preview-image")
                .ui_container()
                .w_full()
                .child(
                    img(ImageSource::Image(image))
                        .w_full()
                        .object_fit(ObjectFit::ScaleDown),
                )
                .into_any_element()
        },
    );
    v_flex()
        .id("preview-picture")
        .flex_1()
        .min_h_0()
        .gap_2()
        .p_2()
        .child(picture)
        .child(
            div()
                .id("preview-image-caption")
                .ui_observe_as(Facts {
                    role: Role::Label,
                    value: Some(caption.clone().into()),
                    ..Facts::button("Image size")
                })
                .type_caption(cx)
                .text_color(cx.theme().muted_foreground)
                .child(caption),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_markdown_image_resolves_to_nothing() {
        let resolve = |_: &gpui_kit::SharedUri| ImageSource::Image(Arc::new(Image::empty()));
        let uri = gpui_kit::SharedUri::from("https://example.com/pixel.png");
        let ImageSource::Image(image) = resolve(&uri) else {
            panic!("a document image must not become a resource")
        };
        assert!(image.bytes.is_empty());
    }

    #[test]
    fn link_schemes_are_limited_to_the_web() {
        assert!(is_web("https://example.com"));
        assert!(is_web("http://localhost:3000"));
        assert!(!is_web("file:///etc/passwd"));
        assert!(!is_web("javascript:alert(1)"));
    }
}
