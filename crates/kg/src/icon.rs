use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::{NSApplication, NSImage};
use objc2_foundation::NSData;

const ICON: &[u8] = include_bytes!("../../../assets/brand/png/kagan-dock-icon-1024.png");

pub(crate) fn install() {
    let Some(marker) = MainThreadMarker::new() else {
        eprintln!("kg: the Dock icon was not set; AppKit icon changes run on the main thread");
        return;
    };
    let Some(image) = image() else {
        eprintln!("kg: the Dock icon could not be read from the embedded PNG");
        return;
    };
    let app = NSApplication::sharedApplication(marker);
    // SAFETY: `image` is an NSImage decoded from the embedded PNG. AppKit's
    // setter may reject nil; this passes a real image. A packaged `.app`
    // would take its icon from the bundle instead, which is a follow-up.
    unsafe { app.setApplicationIconImage(Some(&image)) };
}

fn image() -> Option<objc2::rc::Retained<NSImage>> {
    // SAFETY: `ICON` is a valid byte slice for this call. `dataWithBytes:length:`
    // copies those bytes into the new NSData before returning.
    let data = unsafe { NSData::dataWithBytes_length(ICON.as_ptr().cast(), ICON.len()) };
    NSImage::initWithData(NSImage::alloc(), &data)
}
