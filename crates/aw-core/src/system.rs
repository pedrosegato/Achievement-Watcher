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

#[cfg(windows)]
pub(crate) fn registry_string(
    hkey: windows::Win32::System::Registry::HKEY,
    subkey: windows::core::PCWSTR,
    value: windows::core::PCWSTR,
) -> Option<String> {
    use windows::Win32::System::Registry::{RRF_RT_REG_SZ, RegGetValueW};

    let mut size: u32 = 0;
    // SAFETY: valid constant strings, size query with no data buffer.
    let status = unsafe {
        RegGetValueW(
            hkey,
            subkey,
            value,
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut size),
        )
    };
    if status.is_err() || size < 2 {
        return None;
    }
    let mut buffer = vec![0u16; (size as usize).div_ceil(2)];
    // SAFETY: `buffer` holds `size` bytes and outlives the call.
    let status = unsafe {
        RegGetValueW(
            hkey,
            subkey,
            value,
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if status.is_err() {
        return None;
    }
    let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    let text = String::from_utf16_lossy(&buffer[..len]);
    (!text.trim().is_empty()).then_some(text)
}

#[cfg(windows)]
pub fn machine_guid() -> Option<String> {
    use windows::Win32::System::Registry::HKEY_LOCAL_MACHINE;
    use windows::core::w;
    registry_string(
        HKEY_LOCAL_MACHINE,
        w!(r"SOFTWARE\Microsoft\Cryptography"),
        w!("MachineGuid"),
    )
}

#[cfg(not(windows))]
pub fn machine_guid() -> Option<String> {
    None
}
