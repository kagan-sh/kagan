use std::path::Path;

use gpui_kit::Window;
use kg_domain::OpError;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

pub(super) fn hwnd(window: &Window) -> Result<isize, OpError> {
    let handle = HasWindowHandle::window_handle(window)
        .map_err(|error| OpError::Failed(error.to_string()))?;
    match handle.as_raw() {
        RawWindowHandle::Win32(win32) => Ok(win32.hwnd.get()),
        _ => Err(OpError::Failed("not a Win32 window".into())),
    }
}

pub(super) fn capture(target: isize, path: &Path) -> Result<(), OpError> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC,
        DeleteObject, GetDIBits, GetWindowDC, HGDIOBJ, ReleaseDC, SelectObject,
    };
    use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, PW_RENDERFULLCONTENT};

    let hwnd = HWND(target as _);
    let mut rect = windows::Win32::Foundation::RECT::default();
    // SAFETY: `hwnd` is GPUI's live window handle and `rect` is a writable RECT.
    unsafe { GetWindowRect(hwnd, &raw mut rect) }
        .map_err(|error| OpError::Failed(error.to_string()))?;
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width <= 0 || height <= 0 {
        return Err(OpError::Failed("window has no size".into()));
    }
    let bi_size = u32::try_from(std::mem::size_of::<BITMAPINFOHEADER>())
        .map_err(|_| OpError::Failed("bitmap header is too large".into()))?;
    // SAFETY: `hwnd` is a live window handle; the DC is released below.
    let hdc = unsafe { GetWindowDC(Some(hwnd)) };
    if hdc.is_invalid() {
        return Err(OpError::Failed("GetWindowDC failed".into()));
    }
    // SAFETY: `hdc` was checked above; each handle made here is released below.
    let mem_dc = unsafe { CreateCompatibleDC(Some(hdc)) };
    // SAFETY: `hdc` was checked above; the bitmap is released below.
    let bitmap = unsafe { CreateCompatibleBitmap(hdc, width, height) };
    if mem_dc.is_invalid() || bitmap.is_invalid() {
        // SAFETY: each handle that exists was created above and is released once here.
        unsafe {
            if !bitmap.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(bitmap.0));
            }
            if !mem_dc.is_invalid() {
                let _ = DeleteDC(mem_dc);
            }
            let _ = ReleaseDC(Some(hwnd), hdc);
        }
        return Err(OpError::Failed(
            "could not create the capture buffers".into(),
        ));
    }
    // SAFETY: both handles were checked above and the bitmap is compatible with the DC.
    let old_bitmap = unsafe { SelectObject(mem_dc, HGDIOBJ(bitmap.0)) };
    let printed =
        // SAFETY: `hwnd` is live and `mem_dc` is a compatible memory DC.
        unsafe { PrintWindow(hwnd, mem_dc, PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT)) };
    let header = BITMAPINFOHEADER {
        biSize: bi_size,
        biWidth: width,
        biHeight: -height,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };
    let mut info = BITMAPINFO {
        bmiHeader: header,
        ..Default::default()
    };
    let stride = (width.cast_unsigned() as usize) * 4;
    let mut pixels = vec![0u8; stride * (height.cast_unsigned() as usize)];
    // SAFETY: `pixels` is writable for `height` rows of `stride` bytes and `info` describes that buffer.
    let lines = unsafe {
        GetDIBits(
            mem_dc,
            bitmap,
            0,
            height.cast_unsigned(),
            Some(pixels.as_mut_ptr().cast()),
            &raw mut info,
            windows::Win32::Graphics::Gdi::DIB_RGB_COLORS,
        )
    };
    // SAFETY: each handle was created above and is released exactly once here. Windows will
    // not delete a bitmap that is still selected, so the old object goes back in first.
    unsafe {
        let _ = SelectObject(mem_dc, old_bitmap);
        let _ = DeleteObject(HGDIOBJ(bitmap.0)); // Cleanup: nothing to undo if it fails.
        let _ = DeleteDC(mem_dc); // Cleanup: nothing to undo if it fails.
        let _ = ReleaseDC(Some(hwnd), hdc); // Cleanup: nothing to undo if it fails.
    }
    printed
        .ok()
        .map_err(|error| OpError::Failed(error.to_string()))?;
    if lines == 0 {
        return Err(OpError::Failed("GetDIBits failed".into()));
    }
    for chunk in pixels.chunks_mut(4) {
        chunk.swap(0, 2);
    }
    let image = image::RgbaImage::from_raw(width.cast_unsigned(), height.cast_unsigned(), pixels)
        .ok_or_else(|| OpError::Failed("screenshot buffer is invalid".into()))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    image
        .save(path)
        .map_err(|error| OpError::Failed(error.to_string()))?;
    Ok(())
}
