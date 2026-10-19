//! The Windows (Explorer) context menu for items that share a parent folder, or for a folder's background
//! when there are none. Built and shown on one thread for the life of the process: a dialog a command
//! opens, such as Properties, closes when the thread that invoked it ends, and the submenus (Send to, New)
//! draw themselves through messages to the window that owns the menu.
use std::{cell::{Cell, RefCell}, sync::mpsc, thread};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_SHIFT};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    CMF_EXTENDEDVERBS, CMF_NORMAL, CMIC_MASK_CONTROL_DOWN, CMIC_MASK_PTINVOKE, CMIC_MASK_SHIFT_DOWN,
    CMINVOKECOMMANDINFO, CMINVOKECOMMANDINFOEX, GCS_VERBW, IContextMenu,
    IContextMenu2, IContextMenu3, ILClone, ILFindLastID, ILIsParent, ILRemoveLastID,
    IShellFolder, SHBindToObject, SHBindToParent,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, CreateWindowExW, DefWindowProcW, DeleteMenu, DestroyMenu, DispatchMessageW,
    GetMenuItemCount, GetMenuItemID, GetMessageW, HMENU, MF_BYPOSITION, MSG, PostMessageW,
    RegisterClassW, SW_SHOWNORMAL, SetForegroundWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    TrackPopupMenuEx, TranslateMessage, WINDOW_EX_STYLE, WM_APP, WM_DRAWITEM, WM_INITMENUPOPUP,
    WM_MEASUREITEM, WM_MENUCHAR, WM_NULL, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::{Error, HRESULT, Interface, PCSTR, PCWSTR, PSTR, Result, w};

use super::{Apartment, Owner, Pidls, parse};

/// The bindings leave this one out; it is `SEE_MASK_UNICODE`'s bit.
const CMIC_MASK_UNICODE: u32 = 0x4000;

/// The menu to show: for `paths` (all in one folder), or for the folder `background` when `paths` is empty.
pub struct ShowMenu {
    pub owner: Owner,
    pub paths: Vec<String>,
    pub background: String,
    /// Where to show it, in screen pixels.
    pub at: (i32, i32),
}

struct Request {
    menu: ShowMenu,
    done: mpsc::Sender<Result<bool>>,
}

/// The thread every context menu is shown on.
#[derive(Clone)]
pub struct MenuThread(isize);

impl MenuThread {
    pub fn spawn() -> Result<Self> {
        let (ready, window) = mpsc::channel();
        thread::Builder::new()
            .name("shell-menu".into())
            .spawn(move || run(ready))
            .map_err(|_| Error::from_thread())?;
        window.recv().map_err(|_| Error::empty())?.map(Self)
    }

    /// Shows the menu and carries out what is picked; `Ok(false)` when nothing is.
    /// Blocks until the menu closes, so never call it from the thread that owns `menu.owner`.
    pub fn show(&self, menu: ShowMenu) -> Result<bool> {
        let (done, result) = mpsc::channel();
        let request = Box::into_raw(Box::new(Request { menu, done }));
        let window = HWND(self.0 as *mut _);
        if let Err(error) = unsafe { PostMessageW(Some(window), WM_APP, WPARAM(0), LPARAM(request as isize)) } {
            drop(unsafe { Box::from_raw(request) });
            return Err(error);
        }
        result.recv().map_err(|_| Error::empty())?
    }
}

thread_local! {
    static SHOWN: RefCell<(Option<IContextMenu2>, Option<IContextMenu3>)> = const { RefCell::new((None, None)) };
    /// A menu is up, or a command picked from it is running: a dialog it opens runs its own message loop,
    /// which would hand a second request to this thread in the middle of the first.
    static BUSY: Cell<bool> = const { Cell::new(false) };
}

/// What [`MenuThread::show`] fails with while the thread is still busy with the menu before.
pub const BUSY_ERROR: HRESULT = HRESULT(0x8007_00AA_u32 as i32);

fn run(ready: mpsc::Sender<Result<isize>>) {
    let apartment = Apartment::enter();
    if let Err(error) = apartment.result() {
        let _ = ready.send(Err(error));
        return;
    }
    let window = match create_window() {
        Ok(window) => window,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    let _ = ready.send(Ok(window.0 as isize));
    let mut message = MSG::default();
    while unsafe { GetMessageW(&mut message, None, 0, 0) }.0 > 0 {
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

fn create_window() -> Result<HWND> {
    let instance = unsafe { GetModuleHandleW(None)? };
    let class = WNDCLASSW {
        lpfnWndProc: Some(procedure),
        hInstance: instance.into(),
        lpszClassName: w!("ColetShellMenu"),
        ..Default::default()
    };
    if unsafe { RegisterClassW(&class) } == 0 {
        return Err(Error::from_thread());
    }
    // Not message-only: the menu needs a window that can come to the foreground.
    unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WINDOW_EX_STYLE(0),
            class.lpszClassName,
            PCWSTR::null(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(class.hInstance),
            None,
        )
    }
}

unsafe extern "system" fn procedure(window: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if message == WM_APP {
        let request = unsafe { Box::from_raw(l.0 as *mut Request) };
        if BUSY.replace(true) {
            let _ = request.done.send(Err(Error::from_hresult(BUSY_ERROR)));
            return LRESULT(0);
        }
        let result = show(window, &request.menu);
        BUSY.set(false);
        let _ = request.done.send(result);
        return LRESULT(0);
    }
    if matches!(message, WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR) {
        let handled = SHOWN.with_borrow(|(two, three)| {
            if let Some(three) = three {
                let mut out = LRESULT(0);
                if unsafe { three.HandleMenuMsg2(message, w, l, Some(&mut out)) }.is_ok() {
                    return Some(out);
                }
            }
            if let Some(two) = two
                && message != WM_MENUCHAR
                && unsafe { two.HandleMenuMsg(message, w, l) }.is_ok()
            {
                return Some(LRESULT(if message == WM_INITMENUPOPUP { 0 } else { 1 }));
            }
            None
        });
        if let Some(out) = handled {
            return out;
        }
    }
    unsafe { DefWindowProcW(window, message, w, l) }
}

fn show(window: HWND, request: &ShowMenu) -> Result<bool> {
    let owner = HWND(request.owner as *mut _);
    let menu = context_menu(owner, &request.paths, &request.background)?;
    let popup = unsafe { CreatePopupMenu()? };
    let result = fill(&menu, popup).and_then(|()| {
        SHOWN.set((menu.cast().ok(), menu.cast().ok()));
        // The usual dance so a click elsewhere dismisses the menu.
        let (x, y) = request.at;
        let command = unsafe {
            let _ = SetForegroundWindow(window);
            let command = TrackPopupMenuEx(popup, (TPM_RETURNCMD | TPM_RIGHTBUTTON).0, x, y, window, None);
            let _ = PostMessageW(Some(window), WM_NULL, WPARAM(0), LPARAM(0));
            command
        };
        SHOWN.set((None, None));
        unsafe {
            let _ = SetForegroundWindow(owner);
        }
        if command.0 == 0 {
            return Ok(false);
        }
        invoke(&menu, owner, command.0 as usize - 1, POINT { x, y }).map(|()| true)
    });
    unsafe {
        let _ = DestroyMenu(popup);
    }
    result
}

fn invoke(menu: &IContextMenu, owner: HWND, command: usize, at: POINT) -> Result<()> {
    let mut mask = CMIC_MASK_UNICODE | CMIC_MASK_PTINVOKE;
    unsafe {
        if GetKeyState(VK_CONTROL.0 as i32) < 0 {
            mask |= CMIC_MASK_CONTROL_DOWN;
        }
        if GetKeyState(VK_SHIFT.0 as i32) < 0 {
            mask |= CMIC_MASK_SHIFT_DOWN;
        }
    }
    let info = CMINVOKECOMMANDINFOEX {
        cbSize: size_of::<CMINVOKECOMMANDINFOEX>() as u32,
        fMask: mask,
        hwnd: owner,
        lpVerb: PCSTR(command as *const u8),
        lpVerbW: PCWSTR(command as *const u16),
        nShow: SW_SHOWNORMAL.0,
        ptInvoke: at,
        ..Default::default()
    };
    unsafe { menu.InvokeCommand(&info as *const _ as *const CMINVOKECOMMANDINFO) }
}

fn context_menu(owner: HWND, paths: &[String], background: &str) -> Result<IContextMenu> {
    if paths.is_empty() {
        let folder = Pidls(vec![parse(background)?]);
        let view: IShellFolder = unsafe { SHBindToObject(None, folder.0[0], None)? };
        return unsafe { view.CreateViewObject(owner) };
    }
    let items = Pidls(paths.iter().map(|path| parse(path)).collect::<Result<_>>()?);
    // One menu speaks for items in one folder only.
    let parent = Pidls(vec![unsafe { ILClone(items.0[0]) }]);
    if parent.0[0].is_null() || !unsafe { ILRemoveLastID(Some(parent.0[0])) }.as_bool() {
        return Err(Error::from_hresult(windows::Win32::Foundation::E_INVALIDARG));
    }
    if items.0[1..].iter().any(|item| !unsafe { ILIsParent(parent.0[0], *item, true) }.as_bool()) {
        return Err(Error::from_hresult(windows::Win32::Foundation::E_INVALIDARG));
    }
    let folder: IShellFolder = unsafe { SHBindToParent(items.0[0], None)? };
    let children: Vec<*const ITEMIDLIST> =
        items.0.iter().map(|item| unsafe { ILFindLastID(*item) } as *const _).collect();

    unsafe { folder.GetUIObjectOf(owner, &children, None) }

}

/// Fills `popup` with everything Explorer shows with Shift held, less Rename, which only Explorer's own
/// view can carry out.
fn fill(menu: &IContextMenu, popup: HMENU) -> Result<()> {
    unsafe { menu.QueryContextMenu(popup, 0, 1, 0x7FFF, CMF_NORMAL | CMF_EXTENDEDVERBS).ok()? };
    for position in (0..unsafe { GetMenuItemCount(Some(popup)) }).rev() {
        if verb(menu, popup, position).is_some_and(|verb| verb.eq_ignore_ascii_case("rename")) {
            unsafe { DeleteMenu(popup, position as u32, MF_BYPOSITION)? };
        }
    }
    Ok(())
}

fn verb(menu: &IContextMenu, popup: HMENU, position: i32) -> Option<String> {
    let id = unsafe { GetMenuItemID(popup, position) };
    if !(1..=0x7FFF).contains(&id) {
        return None;
    }
    let mut verb = [0u16; 64];
    unsafe { menu.GetCommandString(id as usize - 1, GCS_VERBW, None, PSTR(verb.as_mut_ptr().cast()), verb.len() as u32) }
        .ok()?;
    let len = verb.iter().position(|&c| c == 0).unwrap_or(verb.len());
    (len > 0).then(|| String::from_utf16_lossy(&verb[..len]))
}

#[cfg(test)]
/// The verbs the menu for `paths` (or `background`) would offer, without showing it.
pub fn verbs(paths: &[String], background: &str) -> Result<Vec<String>> {
    let apartment = Apartment::enter();
    apartment.result()?;
    let menu = context_menu(HWND::default(), paths, background)?;
    let popup = unsafe { CreatePopupMenu()? };
    let result = fill(&menu, popup).map(|()| {
        (0..unsafe { GetMenuItemCount(Some(popup)) }).filter_map(|i| verb(&menu, popup, i)).collect()
    });
    unsafe {
        let _ = DestroyMenu(popup);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offers_the_shell_verbs_less_rename() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txt");
        std::fs::write(&file, "a").unwrap();
        let verbs = verbs(&[file.to_string_lossy().into_owned()], "").unwrap();
        assert!(verbs.iter().any(|verb| verb.eq_ignore_ascii_case("delete")), "{verbs:?}");
        assert!(!verbs.iter().any(|verb| verb.eq_ignore_ascii_case("rename")), "{verbs:?}");
        let background = verbs_for_background(dir.path());
        assert!(!background.is_empty());
    }

    fn verbs_for_background(dir: &std::path::Path) -> Vec<String> {
        verbs(&[], &dir.to_string_lossy()).unwrap()
    }

    #[test]
    fn refuses_items_from_different_folders() {
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let paths = [a.path(), b.path()].map(|dir| {
            let file = dir.join("x.txt");
            std::fs::write(&file, "x").unwrap();
            file.to_string_lossy().into_owned()
        });
        assert!(verbs(&paths, "").is_err());
    }
}
