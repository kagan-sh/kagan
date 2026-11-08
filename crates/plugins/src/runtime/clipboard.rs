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
mod tests {
    use super::{CAP, png};

    #[test]
    fn a_png_header_is_accepted_and_other_bytes_are_not() {
        let header = png("iVBORw0KGgo=").expect("png header");
        assert_eq!(header, [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n']);
        assert!(png("aGVsbG8hISE=").is_err());
        assert!(png("not base64").is_err());
    }

    #[test]
    fn an_8_mib_png_is_accepted_and_one_more_byte_is_not() {
        for size in [CAP - 1, CAP] {
            let encoded = encode(&png_bytes(size));
            let decoded = png(&encoded).expect("within 8 MiB");
            assert_eq!(decoded.len(), size);
        }
        assert!(png(&encode(&png_bytes(CAP + 1))).is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn png_clipboard_is_not_available_on_linux() {
        let arguments =
            gpui_shell::HostArguments::new([gpui_shell::HostValue::Str("iVBORw0KGgo=".into())]);
        let error = super::write(&kg_domain::PluginId("dev.test.png".into()), &arguments)
            .expect_err("linux");
        assert_eq!(
            error.to_string(),
            "png clipboard is not available on this platform"
        );
    }

    fn png_bytes(size: usize) -> Vec<u8> {
        let mut bytes = vec![0; size];
        bytes[..8].copy_from_slice(&[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n']);
        bytes
    }

    fn encode(bytes: &[u8]) -> String {
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        fn piece(packed: u32, shift: u32) -> char {
            let index = usize::try_from((packed >> shift) & 63).unwrap_or(0);
            char::from(TABLE[index])
        }
        let mut out = String::new();
        let mut index = 0;
        while index + 3 <= bytes.len() {
            let packed = (u32::from(bytes[index]) << 16)
                | (u32::from(bytes[index + 1]) << 8)
                | u32::from(bytes[index + 2]);
            out.push(piece(packed, 18));
            out.push(piece(packed, 12));
            out.push(piece(packed, 6));
            out.push(piece(packed, 0));
            index += 3;
        }
        let rest = bytes.len() - index;
        if rest == 1 {
            let packed = u32::from(bytes[index]) << 16;
            out.push(piece(packed, 18));
            out.push(piece(packed, 12));
            out.push('=');
            out.push('=');
        } else if rest == 2 {
            let packed = (u32::from(bytes[index]) << 16) | (u32::from(bytes[index + 1]) << 8);
            out.push(piece(packed, 18));
            out.push(piece(packed, 12));
            out.push(piece(packed, 6));
            out.push('=');
        }
        out
    }
}
