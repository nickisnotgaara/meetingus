#[cfg(windows)]
pub fn apply(title: &str, invisible: bool) {
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_NONE,
    };

    unsafe {
        let mut wide: Vec<u16> = title.encode_utf16().collect();
        wide.push(0);
        let window = FindWindowW(
            windows::core::PCWSTR::null(),
            windows::core::PCWSTR(wide.as_ptr()),
        );
        match window {
            Ok(handle) if !handle.0.is_null() => {
                let affinity = if invisible {
                    WDA_EXCLUDEFROMCAPTURE
                } else {
                    WDA_NONE
                };
                if let Err(error) = SetWindowDisplayAffinity(handle, affinity) {
                    tracing::warn!("SetWindowDisplayAffinity failed: {error:?}");
                } else {
                    tracing::info!("content protection set: invisible={invisible}");
                }
            }
            _ => tracing::warn!("overlay window not found yet - retrying on next toggle"),
        }
    }
}

#[cfg(windows)]
fn find_window(title: &str) -> Option<windows::Win32::Foundation::HWND> {
    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;

    unsafe {
        let mut wide: Vec<u16> = title.encode_utf16().collect();
        wide.push(0);
        let window = FindWindowW(
            windows::core::PCWSTR::null(),
            windows::core::PCWSTR(wide.as_ptr()),
        );
        match window {
            Ok(handle) if !handle.0.is_null() => Some(handle),
            _ => None,
        }
    }
}

#[cfg(windows)]
pub fn ensure_visible(title: &str) {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, GetWindowRect, SetWindowPos, SM_CXSCREEN, SM_CXVIRTUALSCREEN,
        SM_CYSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SWP_NOACTIVATE,
        SWP_NOSIZE, SWP_NOZORDER,
    };

    let Some(handle) = find_window(title) else {
        return;
    };
    unsafe {
        let mut rect = RECT::default();
        if GetWindowRect(handle, &mut rect).is_err() {
            return;
        }
        let vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let vw = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let vh = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        let cx = (rect.left + rect.right) / 2;
        let cy = (rect.top + rect.bottom) / 2;
        let inside = cx >= vx && cx < vx + vw && cy >= vy && cy < vy + vh;
        if !inside {
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;
            let x = (GetSystemMetrics(SM_CXSCREEN) - width) / 2;
            let y = (GetSystemMetrics(SM_CYSCREEN) - height) / 2;
            let _ = SetWindowPos(
                handle,
                None,
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
            tracing::warn!(
                "window was off-screen at ({}, {}) - repositioned to ({}, {})",
                rect.left,
                rect.top,
                x,
                y
            );
        }
    }
}

#[cfg(windows)]
pub fn center(title: &str) {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, GetWindowRect, SetWindowPos, SM_CXSCREEN, SM_CYSCREEN, SWP_NOACTIVATE,
        SWP_NOSIZE, SWP_NOZORDER,
    };

    let Some(handle) = find_window(title) else {
        return;
    };
    unsafe {
        let mut rect = RECT::default();
        if GetWindowRect(handle, &mut rect).is_err() {
            return;
        }
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        let x = (GetSystemMetrics(SM_CXSCREEN) - width) / 2;
        let y = (GetSystemMetrics(SM_CYSCREEN) - height) / 2;
        let _ = SetWindowPos(
            handle,
            None,
            x,
            y,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
        tracing::info!("window centered at ({x}, {y})");
    }
}

#[cfg(windows)]
pub fn bring_to_front(title: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{SetForegroundWindow, ShowWindow, SW_RESTORE};

    if let Some(handle) = find_window(title) {
        unsafe {
            let _ = ShowWindow(handle, SW_RESTORE);
            let _ = SetForegroundWindow(handle);
        }
    }
}

#[cfg(windows)]
pub fn move_by(title: &str, dx: i32, dy: i32) {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, SetWindowPos, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
    };

    let Some(handle) = find_window(title) else {
        tracing::warn!("overlay window not found - cannot move");
        return;
    };
    unsafe {
        let mut rect = RECT::default();
        if GetWindowRect(handle, &mut rect).is_ok() {
            let _ = SetWindowPos(
                handle,
                None,
                rect.left + dx,
                rect.top + dy,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
}

#[cfg(not(windows))]
pub fn move_by(_title: &str, _dx: i32, _dy: i32) {
    tracing::warn!("move_window is only implemented on Windows");
}

#[cfg(not(windows))]
pub fn ensure_visible(_title: &str) {
    tracing::warn!("ensure_visible is only implemented on Windows");
}

#[cfg(not(windows))]
pub fn center(_title: &str) {
    tracing::warn!("center is only implemented on Windows");
}

#[cfg(not(windows))]
pub fn bring_to_front(_title: &str) {
    tracing::warn!("bring_to_front is only implemented on Windows");
}

#[cfg(not(windows))]
pub fn apply(_title: &str, _invisible: bool) {
    tracing::warn!("content protection is only implemented on Windows");
}
