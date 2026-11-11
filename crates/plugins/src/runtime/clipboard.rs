#[cfg(not(target_os = "linux"))]
use crate::area::with_app;
use gpui_shell::{HostArguments, HostError, HostValue};
use kg_domain::PluginId;

#[cfg(not(target_os = "linux"))]
use crate::PluginHost;

const CAP: usize = 8 * 1024 * 1024;

pub(crate) fn write(plugin: &PluginId, arguments: &HostArguments) -> Result<HostValue, HostError> {
    let bytes = png(arguments.string(0)?)?;
    write_png(plugin, bytes)
}

#[cfg(target_os = "linux")]
fn write_png(_plugin: &PluginId, _bytes: Vec<u8>) -> Result<HostValue, HostError> {
    Err(HostError::new(
        "png clipboard is not available on this platform",
    ))
}

#[cfg(not(target_os = "linux"))]
fn write_png(plugin: &PluginId, bytes: Vec<u8>) -> Result<HostValue, HostError> {
    let plugin = plugin.clone();
    with_app(|cx| {
        let allowed = PluginHost::try_global(cx)
            .is_some_and(|host| host.read(cx).allows_clipboard_write(&plugin));
        if !allowed {
            return Err(HostError::new("clipboard write is not granted"));
        }
        let image = gpui_kit::Image::from_bytes(gpui_kit::ImageFormat::Png, bytes);
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_image(&image));
        Ok(HostValue::Null)
    })?
}

fn png(text: &str) -> Result<Vec<u8>, HostError> {
    // Padding makes `len / 4 * 3` up to two bytes above the decoded length.
    if text.len() / 4 * 3 > CAP.saturating_add(2) {
        return Err(HostError::new("png is over 8 MiB"));
    }
    let bytes = decode(text)?;
    if bytes.len() > CAP {
        return Err(HostError::new("png is over 8 MiB"));
    }
    if bytes.len() < 8 || bytes[..8] != [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'] {
        return Err(HostError::new("clipboard image must be a png"));
    }
    Ok(bytes)
}

fn decode(text: &str) -> Result<Vec<u8>, HostError> {
    fn value(byte: u8) -> Option<u8> {
        match byte {
            b'A'..=b'Z' => Some(byte - b'A'),
            b'a'..=b'z' => Some(byte - b'a' + 26),
            b'0'..=b'9' => Some(byte - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    fn octet(value: u32) -> Result<u8, HostError> {
        u8::try_from(value).map_err(|_| HostError::new("png must be base64"))
    }
    let bytes = text.as_bytes();
    let (chunks, rest) = bytes.as_chunks::<4>();
    if chunks.is_empty() || !rest.is_empty() {
        return Err(HostError::new("png must be base64"));
    }
    let mut out = Vec::with_capacity(chunks.len() * 3);
    for chunk in chunks {
        if chunk[2] == b'=' && chunk[3] != b'=' {
            return Err(HostError::new("png must be base64"));
        }
        let Some(first) = value(chunk[0]) else {
            return Err(HostError::new("png must be base64"));
        };
        let Some(second) = value(chunk[1]) else {
            return Err(HostError::new("png must be base64"));
        };
        let third = if chunk[2] == b'=' {
            0
        } else {
            value(chunk[2]).ok_or_else(|| HostError::new("png must be base64"))?
        };
        let fourth = if chunk[3] == b'=' {
            0
        } else {
            value(chunk[3]).ok_or_else(|| HostError::new("png must be base64"))?
        };
        let packed = (u32::from(first) << 18)
            | (u32::from(second) << 12)
            | (u32::from(third) << 6)
            | u32::from(fourth);
        out.push(octet(packed >> 16)?);
        if chunk[2] != b'=' {
            out.push(octet((packed >> 8) & 0xff)?);
        }
        if chunk[3] != b'=' {
            out.push(octet(packed & 0xff)?);
        }
    }
    Ok(out)
}

#[cfg(not(target_os = "linux"))]
impl PluginHost {
    #[must_use]
    pub(crate) fn allows_clipboard_write(&self, id: &PluginId) -> bool {
        self.active
            .get(id)
            .is_some_and(|active| active.grant.powers.clipboard.write)
    }
}

#[cfg(test)]
#[path = "clipboard_test.rs"]
mod tests;
