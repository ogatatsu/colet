//! QuickLook (QL-Win), which previews a file in a window of its own: told what to show through the named
//! pipe it listens on, one line per connection, `<message>|<path>|`.
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
};

use windows::Win32::Foundation::{CloseHandle, ERROR_FILE_NOT_FOUND, ERROR_PIPE_BUSY, GENERIC_WRITE, HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
use windows::Win32::Storage::FileSystem::{CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_NONE, OPEN_EXISTING, WriteFile};
use windows::Win32::System::Pipes::WaitNamedPipeW;
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::core::PWSTR;

use crate::wide::Wide;

/// What QuickLook is asked to do with a path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Message {
    /// Shows it, or closes the preview when it is showing it already.
    Toggle,
    /// Shows it in place of what the preview shows; nothing while no preview is open.
    Switch,
}

impl Message {
    fn name(self) -> &'static str {
        match self {
            Message::Toggle => "QuickLook.App.PipeMessages.Toggle",
            Message::Switch => "QuickLook.App.PipeMessages.Switch",
        }
    }
}

#[derive(Debug)]
pub enum Error {
    /// No QuickLook listens: it is not running, or not installed.
    NotRunning,
    Failed(windows::core::Error),
}

impl From<windows::core::Error> for Error {
    fn from(error: windows::core::Error) -> Self {
        if error.code() == ERROR_FILE_NOT_FOUND.to_hresult() { Error::NotRunning } else { Error::Failed(error) }
    }
}

/// How long to wait for QuickLook to finish with the line before, as it takes one at a time.
const BUSY_WAIT_MS: u32 = 1000;

struct Request {
    message: Message,
    path: PathBuf,
    done: mpsc::Sender<Result<(), Error>>,
}

/// The thread every message goes from, so they reach QuickLook in the order they were sent.
#[derive(Clone)]
pub struct Sender(mpsc::Sender<Request>);

impl Sender {
    pub fn spawn() -> Self {
        let (requests, received) = mpsc::channel::<Request>();
        thread::Builder::new()
            .name("quick-look".into())
            .spawn(move || {
                let pipe = pipe_name();
                for request in received {
                    let result = match &pipe {
                        Ok(pipe) => send(pipe, request.message, &request.path),
                        Err(error) => Err(Error::Failed(error.clone())),
                    };
                    request.done.send(result).ok();
                }
            })
            .expect("start the QuickLook thread");
        Self(requests)
    }

    /// Queues `message` for `path`, and tells how it went through what it returns.
    pub fn send(&self, message: Message, path: PathBuf) -> mpsc::Receiver<Result<(), Error>> {
        let (done, result) = mpsc::channel();
        self.0.send(Request { message, path, done }).ok();
        result
    }
}

/// The pipe QuickLook listens on, its own per user: named after the user's SID.
fn pipe_name() -> windows::core::Result<String> {
    let mut token = HANDLE::default();
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)? };
    let sid = (|| {
        let mut size = 0;
        // Asked once for the size it needs, which fails as it should.
        let _ = unsafe { GetTokenInformation(token, TokenUser, None, 0, &mut size) };
        // As `u64`s, so that the `TOKEN_USER` at its start is aligned.
        let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
        unsafe { GetTokenInformation(token, TokenUser, Some(buffer.as_mut_ptr().cast()), size, &mut size)? };
        let user = unsafe { &*(buffer.as_ptr() as *const TOKEN_USER) };
        let mut text = PWSTR::null();
        unsafe { ConvertSidToStringSidW(user.User.Sid, &mut text)? };
        let sid = unsafe { text.to_string() };
        unsafe { LocalFree(Some(HLOCAL(text.0.cast()))) };
        sid.map_err(|_| windows::core::Error::empty())
    })();
    unsafe { CloseHandle(token).ok() };
    Ok(format!(r"\\.\pipe\QuickLook.App.Pipe.{}", sid?))
}

fn send(pipe: &str, message: Message, path: &Path) -> Result<(), Error> {
    let name = Wide::new(pipe);
    let connect =
        || unsafe { CreateFileW(name.get(), GENERIC_WRITE.0, FILE_SHARE_NONE, None, OPEN_EXISTING, FILE_FLAGS_AND_ATTRIBUTES(0), None) };
    let handle = match connect() {
        Err(error) if error.code() == ERROR_PIPE_BUSY.to_hresult() => {
            // Still reading the line before: tried again once it takes the next.
            if !unsafe { WaitNamedPipeW(name.get(), BUSY_WAIT_MS) }.as_bool() {
                // It may have closed meanwhile, as much as kept busy.
                return Err(windows::core::Error::from_thread().into());
            }
            connect()?
        }
        result => result?,
    };
    let line = format!("{}|{}|\n", message.name(), path.to_string_lossy());
    let mut written = 0;
    let result = unsafe { WriteFile(handle, Some(line.as_bytes()), Some(&mut written), None) };
    unsafe { CloseHandle(handle).ok() };
    Ok(result?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_pipe_after_the_users_sid() {
        let pipe = pipe_name().unwrap();
        assert!(pipe.starts_with(r"\\.\pipe\QuickLook.App.Pipe.S-1-5-"), "{pipe}");
    }

    #[test]
    #[ignore = "needs QuickLook to be running, and shows its preview"]
    fn shows_a_file_in_quick_look() {
        let file = std::env::current_exe().unwrap();
        let sender = Sender::spawn();
        sender.send(Message::Toggle, file.clone()).recv().unwrap().unwrap();
        std::thread::sleep(std::time::Duration::from_secs(1));
        sender.send(Message::Toggle, file).recv().unwrap().unwrap();
    }
}
