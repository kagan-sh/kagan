use super::*;

fn png(width: u32, height: u32) -> Vec<u8> {
    let mut ihdr = width.to_be_bytes().to_vec();
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    bytes.extend(chunk(*b"IHDR", &ihdr));
    bytes.extend(chunk(*b"IDAT", &[]));
    bytes.extend(chunk(*b"IEND", &[]));
    bytes
}

fn chunk(kind: [u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = u32::try_from(data.len()).unwrap().to_be_bytes().to_vec();
    out.extend_from_slice(&kind);
    out.extend_from_slice(data);
    let mut input = kind.to_vec();
    input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&input).to_be_bytes());
    out
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

#[test]
fn an_image_read_stays_inside_the_root_and_under_the_cap() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let inside = root.path().join("pixel.png");
    let secret = outside.path().join("secret.png");
    std::fs::write(&inside, png(2, 2)).unwrap();
    std::fs::write(&secret, png(2, 2)).unwrap();
    assert!(read_image(Some(root.path()), &inside).is_some());
    assert!(read_image(Some(root.path()), &secret).is_none());
    assert!(read_image(None, &secret).is_some());
    let mut bytes = png(2, 2);
    bytes.resize(usize::try_from(IMAGE_LIMIT).unwrap() + 1, 0);
    let heavy = root.path().join("heavy.png");
    std::fs::write(&heavy, bytes).unwrap();
    assert!(read_image(Some(root.path()), &heavy).is_none());
}
