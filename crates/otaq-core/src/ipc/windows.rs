//! Windows: a named pipe whose security descriptor grants access only to the current user's SID (d0027), with
//! remote clients rejected and the client's account checked again on every connection.
//!
//! A named pipe has one instance per connection: the listener keeps one instance waiting, hands it out when a
//! client connects and creates the next one, always with the same owner-only descriptor.

use std::ffi::c_void;
use std::io;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::time::{Duration, Instant};

use tokio::net::windows::named_pipe::{
    ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
};
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_PIPE_BUSY, HANDLE, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertSecurityDescriptorToStringSecurityDescriptorW, ConvertSidToStringSidW,
    ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo, SDDL_REVISION_1,
    SE_KERNEL_OBJECT,
};
use windows_sys::Win32::Security::{
    DACL_SECURITY_INFORMATION, GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
    TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::System::Pipes::GetNamedPipeClientProcessId;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};

use crate::naming;

pub type ServerStream = NamedPipeServer;
pub type ClientStream = NamedPipeClient;
/// An OS account: its SID in string form (`S-1-5-21-…`).
pub type UserId = String;

/// How long a client keeps trying while every pipe instance is busy (the service creates the next one at once).
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// The pipe that stands for the socket path: `\\.\pipe\otaq-<hash of the path>`. The path is compared
/// case-insensitively, as Windows paths are.
pub fn pipe_name(path: &Path) -> String {
    let key = path.to_string_lossy().to_lowercase();
    // FNV-1a: stable across builds and processes, which `DefaultHasher` doesn't promise.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in key.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!(r"\\.\pipe\{}-{hash:016x}", naming::COMMAND_NAME)
}

/// The service's pipe: the instance waiting for the next client.
pub struct Listener {
    name: String,
    security: OwnerOnly,
    waiting: NamedPipeServer,
}

impl Listener {
    /// Creates the pipe for `path`. Fails if the pipe already exists (`first_pipe_instance`), so no other process
    /// can sit on the name before the service.
    pub fn bind(path: &Path) -> io::Result<Self> {
        let name = pipe_name(path);
        let security = OwnerOnly::for_current_user()?;
        let waiting = create(&name, &security, true)?;
        Ok(Self {
            name,
            security,
            waiting,
        })
    }

    pub async fn accept(&mut self) -> io::Result<ServerStream> {
        let connected = self.waiting.connect().await;
        // Whatever happened to this instance, the next client gets a fresh one.
        let next = create(&self.name, &self.security, false)?;
        let instance = std::mem::replace(&mut self.waiting, next);
        connected.map(|()| instance)
    }

    /// The waiting instance's access list in SDDL (`D:P(A;;FA;;;S-1-5-21-…)`), for the ACL inspection test.
    pub fn dacl_sddl(&self) -> io::Result<String> {
        dacl_sddl(handle(&self.waiting))
    }
}

fn create(name: &str, security: &OwnerOnly, first: bool) -> io::Result<NamedPipeServer> {
    let mut attributes = SECURITY_ATTRIBUTES {
        nLength: u32::try_from(std::mem::size_of::<SECURITY_ATTRIBUTES>()).expect("small"),
        lpSecurityDescriptor: security.0,
        bInheritHandle: 0,
    };
    // SAFETY: `attributes` points at a valid security descriptor that outlives this call.
    unsafe {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .create_with_security_attributes_raw(name, (&raw mut attributes).cast::<c_void>())
    }
}

pub async fn connect(path: &Path) -> io::Result<ClientStream> {
    let name = pipe_name(path);
    let deadline = Instant::now() + BUSY_TIMEOUT;
    loop {
        match ClientOptions::new().open(&name) {
            Err(e)
                if e.raw_os_error() == Some(ERROR_PIPE_BUSY as i32)
                    && Instant::now() < deadline =>
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            other => return other,
        }
    }
}

/// The process on the other end of the pipe (CLI-01).
pub fn peer_pid(stream: &ServerStream) -> Option<u32> {
    let mut pid = 0;
    // SAFETY: a valid pipe handle and an out parameter.
    let ok = unsafe { GetNamedPipeClientProcessId(handle(stream), &mut pid) };
    (ok != 0 && pid != 0).then_some(pid)
}

/// The account of the process on the other end.
pub fn peer_user(stream: &ServerStream) -> io::Result<UserId> {
    let pid = peer_pid(stream).ok_or_else(io::Error::last_os_error)?;
    // SAFETY: plain handle-returning call; the handle is closed below.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return Err(io::Error::last_os_error());
    }
    let user = process_user(process);
    // SAFETY: the handle came from OpenProcess.
    unsafe { CloseHandle(process) };
    user
}

/// The account this process runs as.
pub fn current_user() -> UserId {
    // SAFETY: the pseudo handle of the current process needs no closing.
    process_user(unsafe { GetCurrentProcess() }).expect("the current process's token is readable")
}

fn handle(stream: &impl AsRawHandle) -> HANDLE {
    stream.as_raw_handle().cast()
}

/// The SID string of the account a process runs as.
fn process_user(process: HANDLE) -> io::Result<UserId> {
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: out parameter for a token handle, closed below.
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let sid = token_user_sid(token);
    // SAFETY: the handle came from OpenProcessToken.
    unsafe { CloseHandle(token) };
    sid
}

fn token_user_sid(token: HANDLE) -> io::Result<UserId> {
    let mut len = 0u32;
    // SAFETY: a size query with no buffer; it fails with ERROR_INSUFFICIENT_BUFFER and sets `len`.
    unsafe { GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut len) };
    // u64s keep the buffer aligned for TOKEN_USER.
    let mut buffer = vec![0u64; (len as usize).div_ceil(8)];
    // SAFETY: the buffer holds at least `len` bytes.
    let ok =
        unsafe { GetTokenInformation(token, TokenUser, buffer.as_mut_ptr().cast(), len, &mut len) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the call filled a TOKEN_USER at the start of the buffer.
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    let mut text: *mut u16 = std::ptr::null_mut();
    // SAFETY: the SID points into `buffer`, which is alive; the string is freed with LocalFree.
    if unsafe { ConvertSidToStringSidW(user.User.Sid, &mut text) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(take_local_string(text))
}

/// Reads a NUL-terminated wide string allocated by the system and frees it.
fn take_local_string(text: *mut u16) -> String {
    let mut len = 0;
    // SAFETY: the system returned a NUL-terminated string.
    while unsafe { *text.add(len) } != 0 {
        len += 1;
    }
    // SAFETY: `len` characters were just read.
    let string = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, len) });
    // SAFETY: allocated with LocalAlloc by the system call that returned it.
    unsafe { LocalFree(text.cast()) };
    string
}

/// A security descriptor that grants full access to one account and nobody else: no Administrators, no
/// SYSTEM, no Everyone. `P` (protected) keeps inherited entries out.
pub struct OwnerOnly(PSECURITY_DESCRIPTOR);

// SAFETY: the descriptor is created once, never changed, and freed only on drop.
unsafe impl Send for OwnerOnly {}
// SAFETY: as above; the system only reads it.
unsafe impl Sync for OwnerOnly {}

impl OwnerOnly {
    pub fn for_current_user() -> io::Result<Self> {
        Self::for_user(&current_user())
    }

    fn for_user(sid: &str) -> io::Result<Self> {
        let sddl: Vec<u16> = pipe_sddl(sid).encode_utf16().chain([0]).collect();
        let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: a NUL-terminated SDDL string; the descriptor is freed on drop.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(descriptor))
    }
}

impl Drop for OwnerOnly {
    fn drop(&mut self) {
        // SAFETY: allocated with LocalAlloc by ConvertStringSecurityDescriptorToSecurityDescriptorW.
        unsafe { LocalFree(self.0) };
    }
}

/// The descriptor the pipe is created with: owner and only access entry are `sid`.
pub fn pipe_sddl(sid: &str) -> String {
    format!("O:{sid}D:P(A;;GA;;;{sid})")
}

/// The access list a kernel object carries, in SDDL.
fn dacl_sddl(object: HANDLE) -> io::Result<String> {
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: out parameters; the descriptor is freed below.
    let status = unsafe {
        GetSecurityInfo(
            object,
            SE_KERNEL_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    let result = descriptor_dacl_sddl(descriptor);
    // SAFETY: allocated by GetSecurityInfo.
    unsafe { LocalFree(descriptor) };
    result
}

/// `sddl`'s access list as Windows writes it back: SIDs that have an SDDL alias come back as the alias (the
/// built-in Administrator's `S-1-5-21-…-500` as `LA`), so it compares equal to what [`Listener::dacl_sddl`] reads.
pub fn dacl_sddl_as_written(sddl: &str) -> io::Result<String> {
    let wide: Vec<u16> = sddl.encode_utf16().chain([0]).collect();
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: a NUL-terminated SDDL string; the descriptor is freed below.
    let ok = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    let result = descriptor_dacl_sddl(descriptor);
    // SAFETY: allocated with LocalAlloc by ConvertStringSecurityDescriptorToSecurityDescriptorW.
    unsafe { LocalFree(descriptor) };
    result
}

/// A descriptor's access list in SDDL. The caller frees the descriptor.
fn descriptor_dacl_sddl(descriptor: PSECURITY_DESCRIPTOR) -> io::Result<String> {
    let mut text: *mut u16 = std::ptr::null_mut();
    // SAFETY: a valid descriptor; the string is freed by take_local_string.
    let ok = unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor,
            SDDL_REVISION_1,
            DACL_SECURITY_INFORMATION,
            &mut text,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(take_local_string(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_pipe_per_data_folder() {
        let a = pipe_name(Path::new(r"C:\Users\me\.cb\x.sock"));
        assert!(a.starts_with(r"\\.\pipe\"), "{a}");
        assert_eq!(a, pipe_name(Path::new(r"c:\users\ME\.cb\x.sock")));
        assert_ne!(a, pipe_name(Path::new(r"C:\Temp\home\x.sock")));
    }

    #[test]
    fn the_current_user_is_a_sid() {
        assert!(current_user().starts_with("S-1-"), "{}", current_user());
    }
}
