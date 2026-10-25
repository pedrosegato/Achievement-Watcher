#[cfg(windows)]
fn read_dword(subkey: windows::core::PCWSTR, value: windows::core::PCWSTR) -> Option<u32> {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};

    let mut data: u32 = 0;
    let mut size = u32::try_from(std::mem::size_of::<u32>()).ok()?;
    // SAFETY: `data` is a valid u32 buffer of `size` bytes that outlives the call.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey,
            value,
            RRF_RT_REG_DWORD,
            None,
            Some(std::ptr::from_mut(&mut data).cast()),
            Some(&mut size),
        )
    };
    status.is_ok().then_some(data)
}

#[cfg(windows)]
pub fn accent_color_dword() -> Option<u32> {
    use windows::core::w;
    read_dword(w!(r"Software\Microsoft\Windows\DWM"), w!("AccentColor"))
}

#[cfg(windows)]
pub fn apps_use_light_theme() -> Option<bool> {
    use windows::core::w;
    read_dword(
        w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
        w!("AppsUseLightTheme"),
    )
    .map(|value| value == 1)
}

#[cfg(not(windows))]
pub fn accent_color_dword() -> Option<u32> {
    None
}

#[cfg(not(windows))]
pub fn apps_use_light_theme() -> Option<bool> {
    None
}
