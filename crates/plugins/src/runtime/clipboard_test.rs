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
    let error =
        super::write(&kg_domain::PluginId("dev.test.png".into()), &arguments).expect_err("linux");
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
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
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
