use anyhow::Result;

#[cfg(target_os = "windows")]
const RUN_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(target_os = "windows")]
const APP_VALUE: &str = "Slipkey";

#[cfg(target_os = "windows")]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn is_enabled() -> bool {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::System::Registry::{
            RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY_CURRENT_USER, KEY_QUERY_VALUE,
        };

        let mut hkey = std::ptr::null_mut();
        let ret = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                wide(RUN_SUBKEY).as_ptr(),
                0,
                KEY_QUERY_VALUE,
                &mut hkey,
            )
        };
        if ret != 0 {
            return false;
        }
        let mut data_type = 0u32;
        let mut data_size = 0u32;
        let ret = unsafe {
            RegQueryValueExW(
                hkey,
                wide(APP_VALUE).as_ptr(),
                std::ptr::null_mut(),
                &mut data_type,
                std::ptr::null_mut(),
                &mut data_size,
            )
        };
        unsafe { RegCloseKey(hkey) };
        ret == 0
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

pub fn set_enabled(enabled: bool) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        use anyhow::Context as _;
        use windows_sys::Win32::System::Registry::{
            RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY_CURRENT_USER,
            KEY_SET_VALUE, REG_SZ,
        };

        let mut hkey = std::ptr::null_mut();
        let ret = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                wide(RUN_SUBKEY).as_ptr(),
                0,
                KEY_SET_VALUE,
                &mut hkey,
            )
        };
        if ret != 0 {
            anyhow::bail!("RegOpenKeyExW failed: {ret}");
        }
        let value = wide(APP_VALUE);
        let result = if enabled {
            let exe = std::env::current_exe().context("current_exe")?;
            let path = wide(&exe.to_string_lossy());
            unsafe {
                RegSetValueExW(
                    hkey,
                    value.as_ptr(),
                    0,
                    REG_SZ,
                    path.as_ptr() as *const u8,
                    (path.len() * 2) as u32,
                )
            }
        } else {
            unsafe { RegDeleteValueW(hkey, value.as_ptr()) }
        };
        unsafe { RegCloseKey(hkey) };
        if result != 0 && !(result == 2 && !enabled) {
            anyhow::bail!("registry op failed: {result}");
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = enabled;
        Ok(())
    }
}
