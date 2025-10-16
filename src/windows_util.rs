// Windows-specific utility helpers. Currently only provides a helper to disable
// DWM transition animations (fade) for instant show/hide UX. This is internal
// and not part of the public API surface.

#[cfg(target_os = "windows")]
pub fn disable_window_transitions(window: &winit::window::Window) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_TRANSITIONS_FORCEDISABLED};
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    if let Ok(h) = window.window_handle() {
        let raw = h.as_raw();
        if let RawWindowHandle::Win32(win) = raw {
            unsafe {
                let hwnd = HWND(win.hwnd.get() as *mut _);
                let value: i32 = 1;
                let hr = DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_TRANSITIONS_FORCEDISABLED,
                    &value as *const _ as *const _,
                    std::mem::size_of_val(&value) as u32,
                );
                #[allow(unused)]
                {
                    if hr.is_ok() {
                        log::debug!("disabled DWM transitions for window: {:?}", hwnd);
                    } else {
                        log::debug!(
                            "failed to disable DWM transitions (hr={hr:?}) for window: {:?}",
                            hwnd
                        );
                    }
                }
            }
        }
    }
}
