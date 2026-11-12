use std::borrow::Cow;
#[cfg(target_os = "linux")]
use std::sync::{Arc, OnceLock};

use gpui_kit::component::Icon;
use gpui_kit::{AssetSource, Hsla, IntoElement, SharedString, Styled, img, px, svg};

// Desktop id for the Linux window. Matches the notification identity.
#[cfg(target_os = "linux")]
const APP_ID: &str = "kagan.kg";

const MARK_BLACK: &str = "brand/kagan-mark-black.svg";
const MARK_WHITE: &str = "brand/kagan-mark-white.svg";
const WORDMARK_BLACK: &str = "brand/kagan-logo-black.svg";
const WORDMARK_WHITE: &str = "brand/kagan-logo-white.svg";
const APP_ICON: &str = "brand/png/kagan-app-icon-256.png";

pub(crate) fn mark(color: Hsla, size: f32) -> impl IntoElement {
    svg().path(MARK_BLACK).size(px(size)).text_color(color)
}

pub(crate) fn mark_icon() -> Icon {
    Icon::empty().path(MARK_BLACK)
}

pub(crate) fn app_icon(size: f32) -> impl IntoElement {
    img(APP_ICON).size(px(size))
}

const BRAND: &[(&str, &[u8])] = &[
    (
        MARK_BLACK,
        include_bytes!("../../../assets/brand/kagan-mark-black.svg"),
    ),
    (
        MARK_WHITE,
        include_bytes!("../../../assets/brand/kagan-mark-white.svg"),
    ),
    (
        WORDMARK_BLACK,
        include_bytes!("../../../assets/brand/kagan-logo-black.svg"),
    ),
    (
        WORDMARK_WHITE,
        include_bytes!("../../../assets/brand/kagan-logo-white.svg"),
    ),
    (
        APP_ICON,
        include_bytes!("../../../assets/brand/png/kagan-app-icon-256.png"),
    ),
    (
        "agents/claude.svg",
        include_bytes!("../../../assets/agents/claude.svg"),
    ),
    (
        "agents/codex.svg",
        include_bytes!("../../../assets/agents/codex.svg"),
    ),
    (
        "agents/gemini.svg",
        include_bytes!("../../../assets/agents/gemini.svg"),
    ),
    (
        "agents/copilot.svg",
        include_bytes!("../../../assets/agents/copilot.svg"),
    ),
    (
        "agents/cursor.svg",
        include_bytes!("../../../assets/agents/cursor.svg"),
    ),
    (
        "agents/kimi.svg",
        include_bytes!("../../../assets/agents/kimi.svg"),
    ),
    (
        "agents/opencode.svg",
        include_bytes!("../../../assets/agents/opencode.svg"),
    ),
];

#[derive(Clone, Copy, Debug, Default)]
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui_kit::Result<Option<Cow<'static, [u8]>>> {
        if let Some(bytes) = brand_bytes(path) {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        gpui_kit::assets::AllAssets.load(path)
    }

    fn list(&self, path: &str) -> gpui_kit::Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::AllAssets.list(path)?;
        for (name, _) in BRAND {
            if name.starts_with(path) {
                paths.push((*name).into());
            }
        }
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

fn brand_bytes(path: &str) -> Option<&'static [u8]> {
    BRAND
        .iter()
        .find(|(name, _)| *name == path)
        .map(|(_, bytes)| *bytes)
}

// GPUI writes `icon` as `_NET_WM_ICON` on X11 only. Wayland sets `app_id`
// and has no window-icon call. A `.desktop` file would supply that icon,
// and a packaged `.app` or `.desktop` entry is a follow-up.
#[must_use]
pub fn prepare_window(
    options: gpui_kit::WindowOptions,
    cx: &gpui_kit::App,
) -> gpui_kit::WindowOptions {
    let mut options = options;
    crate::presentation::apply(&mut options, cx);
    #[cfg(target_os = "linux")]
    {
        let mut options = options;
        if options.app_id.is_none() {
            options.app_id = Some(APP_ID.to_owned());
        }
        if options.icon.is_none() {
            options.icon = window_icon();
        }
        options
    }
    #[cfg(not(target_os = "linux"))]
    {
        options
    }
}

#[cfg(target_os = "linux")]
fn window_icon() -> Option<Arc<image::RgbaImage>> {
    static ICON: OnceLock<Option<Arc<image::RgbaImage>>> = OnceLock::new();
    ICON.get_or_init(|| {
        let png = include_bytes!("../../../assets/brand/png/kagan-app-icon-256.png");
        match image::load_from_memory(png) {
            Ok(image) => Some(Arc::new(image.into_rgba8())),
            Err(error) => {
                eprintln!("kg: the window icon could not be decoded: {error}");
                None
            }
        }
    })
    .clone()
}
