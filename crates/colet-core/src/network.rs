//! A server's shares, and signing in to a server that will not let the user in as they are.
use std::{
    io,
    path::{Path, PathBuf},
    sync::{
        Mutex, PoisonError,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use windows::Win32::{
    Foundation::HWND,
    NetworkManagement::{
        NetManagement::{MAX_PREFERRED_LENGTH, NetApiBufferFree},
        WNet::{CONNECT_INTERACTIVE, NETRESOURCEW, RESOURCETYPE_ANY, WNetAddConnection3W},
    },
    Storage::FileSystem::{NetShareEnum, SHARE_INFO_1, STYPE_DISKTREE, STYPE_MASK},
};
use windows_core::{PCWSTR, PWSTR};

use crate::{
    entry::{Entry, Hiding},
    paths::{ancestors, child, from_wide, is_server, shown},
    shell::Owner,
    wide::Wide,
};

/// Whether `error` says the server will not let the user in as they are signed in, which signing in as
/// someone else may mend: the access denied, a wrong name or password, or no sign-in at all.
fn refused(error: &io::Error) -> bool {
    const ERROR_ACCESS_DENIED: i32 = 5;
    const ERROR_INVALID_PASSWORD: i32 = 86;
    const ERROR_NOT_AUTHENTICATED: i32 = 1244;
    const ERROR_LOGON_FAILURE: i32 = 1326;
    matches!(error.raw_os_error(), Some(ERROR_ACCESS_DENIED | ERROR_INVALID_PASSWORD | ERROR_NOT_AUTHENTICATED | ERROR_LOGON_FAILURE))
}

/// The buffer a network call allocated, freed when it goes.
struct NetBuffer(*mut u8);

impl Drop for NetBuffer {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { NetApiBufferFree(Some(self.0 as *const _)) };
        }
    }
}

/// The disk shares `server` (`\\server`) offers, each a folder named as the server names it: one whose
/// name ends in `$`, as `C$` or `ADMIN$`, is a hidden item, as in Explorer. Printers and the IPC share are
/// never listed. Unsorted.
pub fn shares(server: &Path) -> io::Result<Vec<Entry>> {
    const ERROR_MORE_DATA: u32 = 234;
    let name = Wide::new(server);
    let mut buffer = NetBuffer(std::ptr::null_mut());
    let (mut read, mut total) = (0u32, 0u32);
    let status = unsafe { NetShareEnum(name.get(), 1, &mut buffer.0, MAX_PREFERRED_LENGTH, &mut read, &mut total, None) };
    // Asked for all at once, it never holds some back; should it, those it gave are shown.
    if status != 0 && status != ERROR_MORE_DATA {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    let infos: &[SHARE_INFO_1] =
        if buffer.0.is_null() { &[] } else { unsafe { std::slice::from_raw_parts(buffer.0 as *const SHARE_INFO_1, read as usize) } };
    let parent: std::sync::Arc<str> = shown(server).into();
    Ok(infos
        .iter()
        .filter(|info| info.shi1_type.0 & STYPE_MASK.0 == STYPE_DISKTREE.0)
        .map(|info| {
            let name = from_wide(unsafe { info.shi1_netname.as_wide() });
            let path = child(server, &name);
            let shown_name = shown(&name);
            Entry {
                hidden: if shown_name.ends_with('$') { Hiding::Hidden } else { Hiding::Shown },
                parent: parent.clone(),
                ..Entry::folder(path, &shown_name)
            }
        })
        .collect())
}

/// Whether `server` (`\\server`) answers, though it may not let the user in as they are.
pub fn answers(server: &Path) -> bool {
    match shares(server) {
        Ok(_) => true,
        Err(error) => refused(&error),
    }
}

/// One sign-in is asked at a time.
static ASKING: Mutex<()> = Mutex::new(());
/// How many sign-ins have been asked, for a listing that waited on another's to tell it was.
static ASKED: AtomicU64 = AtomicU64::new(0);

/// What a sign-in to the server holding `path` connects to: the share it is on, or for the server itself
/// its IPC share, as Explorer does.
fn connection(path: &Path) -> Option<PathBuf> {
    let chain = ancestors(path);
    let server = chain.first().filter(|first| is_server(first))?;
    Some(chain.get(1).cloned().unwrap_or_else(|| child(server, "IPC$")))
}

/// Does `attempt`, and when a server refuses the user as they are signed in, asks Windows to sign in to
/// the server that holds `path`, which shows its own dialog for a name and password, then does it again.
/// A listing that waited on another's sign-in tries again without asking a second time. Given up, the
/// sign-in leaves the first error standing, as does `cancelled` set before the dialog shows: no one waits
/// for that listing any more, so no one is asked.
pub fn signing_in<T>(path: &Path, owner: Owner, cancelled: &AtomicBool, attempt: impl Fn() -> io::Result<T>) -> io::Result<T> {
    let seen = ASKED.load(Ordering::SeqCst);
    let error = match attempt() {
        Err(error) if refused(&error) => error,
        done => return done,
    };
    let Some(remote) = connection(path) else { return Err(error) };
    if cancelled.load(Ordering::Relaxed) {
        return Err(error);
    }
    let _asking = ASKING.lock().unwrap_or_else(PoisonError::into_inner);
    // Waited behind another's dialog, it may have been let go of meanwhile.
    if cancelled.load(Ordering::Relaxed) {
        return Err(error);
    }
    if ASKED.load(Ordering::SeqCst) != seen {
        return attempt();
    }
    let remote = Wide::new(remote);
    let resource = NETRESOURCEW { dwType: RESOURCETYPE_ANY, lpRemoteName: PWSTR(remote.get().0 as *mut _), ..Default::default() };
    let status =
        unsafe { WNetAddConnection3W(Some(HWND(owner as *mut _)), &resource, PCWSTR::null(), PCWSTR::null(), CONNECT_INTERACTIVE) };
    ASKED.fetch_add(1, Ordering::SeqCst);
    if status.0 != 0 {
        return Err(error);
    }
    attempt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signs_in_to_the_share_or_for_the_server_its_ipc_share() {
        let connected = |path: &str| connection(Path::new(path)).map(shown);
        assert_eq!(connected(r"\\server").as_deref(), Some(r"\\server\IPC$"));
        assert_eq!(connected(r"\\server\share").as_deref(), Some(r"\\server\share"));
        assert_eq!(connected(r"\\server\share\a\b").as_deref(), Some(r"\\server\share"));
        assert_eq!(connected(r"D:\a"), None);
    }

    #[test]
    fn asks_no_one_where_the_attempt_is_let_in_or_fails_otherwise() {
        let waited = AtomicBool::new(false);
        let found = signing_in(Path::new(r"\\server\share"), 0, &waited, || Ok(1));
        assert_eq!(found.unwrap(), 1);
        let gone = signing_in(Path::new(r"\\server\share"), 0, &waited, || Err::<(), _>(io::Error::from_raw_os_error(53)));
        assert_eq!(gone.unwrap_err().raw_os_error(), Some(53));
        // A folder on a drive is never signed in to.
        let denied = signing_in(Path::new(r"D:\a"), 0, &waited, || Err::<(), _>(io::Error::from_raw_os_error(5)));
        assert_eq!(denied.unwrap_err().raw_os_error(), Some(5));
    }

    #[test]
    fn asks_no_one_for_a_listing_let_go_of() {
        let attempts = std::sync::atomic::AtomicUsize::new(0);
        let refused = signing_in(Path::new(r"\\server\share"), 0, &AtomicBool::new(true), || {
            attempts.fetch_add(1, Ordering::SeqCst);
            Err::<(), _>(io::Error::from_raw_os_error(5))
        });
        assert_eq!(refused.unwrap_err().raw_os_error(), Some(5));
        // Not tried again, as it would be after a sign-in or another's.
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn lists_the_disk_shares_of_this_computer_with_those_ending_in_dollar_hidden() {
        let server = PathBuf::from(r"\\localhost");
        let Ok(shares) = shares(&server) else { return };
        for share in &shares {
            assert_eq!(share.hidden.is_hidden(), share.name.ends_with('$'), "{}", share.name);
            assert_ne!(share.name.to_uppercase(), "IPC$");
            assert_eq!(shown(&share.path), format!(r"\\localhost\{}", share.name));
        }
    }
}
