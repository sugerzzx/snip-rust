//! Single instance helper.
//! Windows: use a named mutex (Global scope) to prevent multiple instances.
//! Other platforms: currently no restriction (always succeeds).

#[cfg(target_os = "windows")]
use windows::core::PCWSTR;
#[cfg(target_os = "windows")]
use windows::Win32::{
    Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE},
    System::Threading::CreateMutexW,
};

#[cfg(target_os = "windows")]
pub struct InstanceGuard(HANDLE);

#[cfg(target_os = "windows")]
impl Drop for InstanceGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Try acquire singleton mutex. Returns Some(guard) if acquired, None if another instance exists.
#[cfg(target_os = "windows")]
pub fn acquire_single_instance() -> Option<InstanceGuard> {
    const NAME: &str = "Global\\SnipRustSingletonMutex";
    let wide: Vec<u16> = NAME.encode_utf16().chain(std::iter::once(0)).collect();
    let result = unsafe { CreateMutexW(None, false, PCWSTR(wide.as_ptr())) };
    let handle = match result {
        Ok(h) => h,
        Err(_) => {
            // 创建失败：不阻止启动
            return Some(InstanceGuard(HANDLE::default()));
        }
    };
    unsafe {
        if GetLastError() == ERROR_ALREADY_EXISTS {
            None
        } else {
            Some(InstanceGuard(handle))
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub struct InstanceGuard;
#[cfg(not(target_os = "windows"))]
pub fn acquire_single_instance() -> Option<InstanceGuard> {
    Some(InstanceGuard)
}
