//! One Colet per logon session. A second start asks the running one to come to the front and quits.
//! Debug builds are counted apart from release ones, so one of each can run side by side.

use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, EVENT_MODIFY_STATE, INFINITE, OpenEventW, SetEvent, WaitForSingleObject,
};
use windows::Win32::UI::WindowsAndMessaging::{
    ASFW_ANY, AllowSetForegroundWindow, IsIconic, SW_RESTORE, SetForegroundWindow, ShowWindow,
};
use windows::core::{PCWSTR, w};

#[cfg(not(debug_assertions))]
const MUTEX: PCWSTR = w!("Local\\Colet-3f6c2a9e-instance");
#[cfg(not(debug_assertions))]
const EVENT: PCWSTR = w!("Local\\Colet-3f6c2a9e-activate");
#[cfg(debug_assertions)]
const MUTEX: PCWSTR = w!("Local\\Colet-3f6c2a9e-instance-debug");
#[cfg(debug_assertions)]
const EVENT: PCWSTR = w!("Local\\Colet-3f6c2a9e-activate-debug");

/// This process is the one Colet. Its handles are never closed, so the claim lasts until the process ends.
pub struct Instance {
    _mutex: HANDLE,
    event: HANDLE,
}

/// Claims the one Colet, or, when one is already running, asks it to the front and returns `None`.
/// One still starting, before its window is up, is left to come up on its own.
pub fn claim() -> Option<Instance> {
    let mutex = unsafe { CreateMutexW(None, false, MUTEX) };
    let already = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    let Ok(mutex) = mutex else {
        // Nothing to check against: run rather than refuse to start.
        return Some(Instance { _mutex: HANDLE::default(), event: HANDLE::default() });
    };
    if already {
        if let Ok(event) = unsafe { OpenEventW(EVENT_MODIFY_STATE, false, EVENT) } {
            // This process was just started by the user, so it may hand the foreground on.
            let _ = unsafe { AllowSetForegroundWindow(ASFW_ANY) };
            let _ = unsafe { SetEvent(event) };
        }
        return None;
    }
    let event = unsafe { CreateEventW(None, false, false, EVENT) }.unwrap_or_default();
    Some(Instance { _mutex: mutex, event })
}

impl Instance {
    /// From now on, brings `hwnd` to the front each time another start asks: restored if minimized,
    /// as it was otherwise.
    pub fn answer(self, hwnd: isize) {
        if self.event.is_invalid() || hwnd == 0 {
            return;
        }
        let event = self.event.0 as isize;
        std::thread::spawn(move || {
            let (event, hwnd) = (HANDLE(event as *mut _), HWND(hwnd as *mut _));
            loop {
                unsafe { WaitForSingleObject(event, INFINITE) };
                unsafe {
                    if IsIconic(hwnd).as_bool() {
                        let _ = ShowWindow(hwnd, SW_RESTORE);
                    }
                    let _ = SetForegroundWindow(hwnd);
                }
            }
        });
    }
}
