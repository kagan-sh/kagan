use std::{io, ptr};

use interprocess::os::windows::security_descriptor::SecurityDescriptor;
use widestring::{U16CStr, U16CString};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, LocalFree},
    Security::{
        Authorization::ConvertSidToStringSidW, GetTokenInformation, TOKEN_QUERY, TOKEN_USER,
        TokenUser,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

pub(crate) fn current_user_only() -> io::Result<SecurityDescriptor> {
    let sddl = U16CString::from_str(format!("D:P(A;;GA;;;{})", current_user_sid()?))
        .map_err(io::Error::other)?;
    SecurityDescriptor::deserialize(&sddl)
}

fn current_user_sid() -> io::Result<String> {
    let mut token: HANDLE = ptr::null_mut();
    // SAFETY: the pseudo handle from GetCurrentProcess needs no closing; `token` is written on success.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut len = 0u32;
    // SAFETY: a null buffer of length 0 only asks for the needed length.
    unsafe { GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &raw mut len) };
    // u64 cells keep the buffer aligned for TOKEN_USER.
    let mut buffer = vec![0u64; (len as usize).div_ceil(8)];
    // SAFETY: `buffer` holds at least `len` bytes.
    let filled = unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            len,
            &raw mut len,
        )
    };
    // SAFETY: `token` is an open handle owned here.
    unsafe { CloseHandle(token) };
    if filled == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: GetTokenInformation filled `buffer` with a TOKEN_USER whose SID lives in `buffer`.
    let sid = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    let mut text = ptr::null_mut();
    // SAFETY: `sid` is valid while `buffer` lives; `text` is written on success.
    if unsafe { ConvertSidToStringSidW(sid, &raw mut text) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `text` is a NUL-terminated string from ConvertSidToStringSidW, freed with LocalFree.
    let sid = unsafe {
        let sid = U16CStr::from_ptr_str(text).to_string_lossy();
        LocalFree(text.cast());
        sid
    };
    Ok(sid)
}
