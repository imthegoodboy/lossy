//! Small Windows API boundary. No keyboard hooks or elevated capture.
#![allow(unsafe_code)]
use std::{os::windows::process::CommandExt, process::Command};
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree},
    Security::{
        Authorization::ConvertSidToStringSidW, GetTokenInformation, TOKEN_QUERY, TOKEN_USER,
        TokenUser,
    },
    System::{
        DataExchange::{GetClipboardOwner, GetClipboardSequenceNumber},
        StationsAndDesktops::{CloseDesktop, DESKTOP_READOBJECTS, OpenInputDesktop},
        Threading::{
            GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32,
            PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
        },
    },
    UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId},
};
use windows::core::PWSTR;

pub fn sid() -> Result<String, String> {
    // SAFETY: token buffer is aligned and kept alive while the SID is converted. Both
    // Windows allocations/handles are released on their owning paths.
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)
            .map_err(|_| "User identity unavailable")?;
        let mut needed = 0;
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut needed);
        let mut storage = vec![0usize; (needed as usize).div_ceil(std::mem::size_of::<usize>())];
        let result = GetTokenInformation(
            token,
            TokenUser,
            Some(storage.as_mut_ptr().cast()),
            needed,
            &mut needed,
        );
        let _ = CloseHandle(token);
        result.map_err(|_| "User identity unavailable")?;
        let user = &*storage.as_ptr().cast::<TOKEN_USER>();
        let mut text = PWSTR::null();
        ConvertSidToStringSidW(user.User.Sid, &mut text)
            .map_err(|_| "User identity unavailable")?;
        let sid = text.to_string().map_err(|_| "User identity unavailable");
        let _ = LocalFree(Some(HLOCAL(text.0.cast())));
        sid.map_err(str::to_owned)
    }
}

pub fn foreground() -> Option<(u32, String, String)> {
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let path = process_path(pid)?;
        let exe = std::path::Path::new(&path)
            .file_name()?
            .to_string_lossy()
            .to_lowercase();
        let mut title = vec![0u16; 1024];
        let n = GetWindowTextW(hwnd, &mut title);
        Some((
            pid,
            exe,
            String::from_utf16_lossy(&title[..n.max(0) as usize]),
        ))
    }
}

fn process_path(pid: u32) -> Option<String> {
    // SAFETY: the query buffer stays alive and the process handle is always closed.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut path = vec![0u16; 32768];
        let mut length = path.len() as u32;
        let ok = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(path.as_mut_ptr()),
            &mut length,
        );
        let _ = CloseHandle(process);
        ok.ok()?;
        Some(String::from_utf16_lossy(&path[..length as usize]))
    }
}
pub fn same_application_process(foreground_pid: u32, element_pid: u32) -> bool {
    if foreground_pid == 0 || element_pid == 0 {
        return false;
    }
    if foreground_pid == element_pid {
        return true;
    }
    // Electron accessibility can belong to a renderer. Require the same full
    // executable path, not a basename match or just any focused process.
    match (process_path(foreground_pid), process_path(element_pid)) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(&b),
        _ => false,
    }
}
pub fn clipboard_sequence() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}
pub fn clipboard_owner_pid() -> Option<u32> {
    // SAFETY: Windows owns the returned HWND; only its process identifier is queried.
    unsafe {
        let owner = GetClipboardOwner().ok()?;
        let mut pid = 0;
        GetWindowThreadProcessId(owner, Some(&mut pid));
        (pid != 0).then_some(pid)
    }
}
pub fn desktop_available() -> bool {
    use windows::Win32::System::StationsAndDesktops::{GetUserObjectInformationW, UOI_NAME};
    unsafe {
        match OpenInputDesktop(Default::default(), false, DESKTOP_READOBJECTS) {
            Ok(handle) => {
                let mut name = [0u16; 256];
                let result = GetUserObjectInformationW(
                    HANDLE(handle.0),
                    UOI_NAME,
                    Some(name.as_mut_ptr().cast()),
                    512,
                    None,
                );
                let _ = CloseDesktop(handle);
                result.is_ok()
                    && String::from_utf16_lossy(
                        &name[..name.iter().position(|v| *v == 0).unwrap_or(name.len())],
                    )
                    .eq_ignore_ascii_case("default")
            }
            Err(_) => false,
        }
    }
}

pub fn spawn_agent() -> Result<(), String> {
    Command::new(std::env::current_exe().map_err(|_| "Executable unavailable")?)
        .arg("--agent")
        .creation_flags(0x08000000)
        .spawn()
        .map_err(|_| "Could not start background process")?;
    Ok(())
}
pub fn open_ui() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = Command::new(exe).creation_flags(0x08000000).spawn();
    }
}

pub fn startup(enable: bool) -> Result<(), String> {
    let task = "Lossy Background Recovery";
    let (run, _) = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")
        .map_err(|_| "Windows startup preferences are unavailable")?;
    if !enable {
        if run.get_raw_value("Lossy").is_ok() {
            run.delete_value("Lossy")
                .map_err(|_| "Could not disable Lossy startup")?;
        }
        let exists = Command::new("schtasks.exe")
            .args(["/Query", "/TN", task])
            .creation_flags(0x08000000)
            .output()
            .is_ok_and(|out| out.status.success());
        if !exists {
            return Ok(());
        }
        let result = Command::new("schtasks.exe")
            .args(["/Delete", "/TN", task, "/F"])
            .creation_flags(0x08000000)
            .output()
            .map_err(|_| "Could not disable the Lossy startup task")?;
        if !result.status.success() {
            return Err("Windows refused to disable the Lossy startup task".into());
        }
        return Ok(());
    }
    let executable = std::env::current_exe().map_err(|_| "Executable unavailable")?;
    let exe = executable
        .to_string_lossy()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let sid = sid()?;
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task"><Triggers><LogonTrigger><Enabled>true</Enabled><UserId>{sid}</UserId></LogonTrigger></Triggers><Principals><Principal id="Author"><UserId>{sid}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals><Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><StopIfGoingOnBatteries>false</StopIfGoingOnBatteries><StartWhenAvailable>true</StartWhenAvailable><ExecutionTimeLimit>PT0S</ExecutionTimeLimit><RestartOnFailure><Interval>PT1M</Interval><Count>3</Count></RestartOnFailure></Settings><Actions Context="Author"><Exec><Command>{exe}</Command><Arguments>--agent</Arguments></Exec></Actions></Task>"#
    );
    use std::io::Write;
    let mut temp =
        tempfile::NamedTempFile::new().map_err(|_| "Startup configuration unavailable")?;
    temp.write_all(xml.as_bytes())
        .map_err(|_| "Startup configuration unavailable")?;
    let out = Command::new("schtasks.exe")
        .args(["/Create", "/TN", task, "/XML"])
        .arg(temp.path())
        .arg("/F")
        .creation_flags(0x08000000)
        .output()
        .map_err(|_| "Could not register startup")?;
    if !out.status.success() {
        // Standard-user policy may reject scheduled tasks. HKCU Run still gives a visible,
        // user-controllable, non-elevated sign-in launch without opening the archive window.
        run.set_value("Lossy", &format!("\"{}\" --agent", executable.display()))
            .map_err(
                |_| "Windows refused startup registration. You can still run Lossy manually.",
            )?;
    } else if run.get_raw_value("Lossy").is_ok() {
        run.delete_value("Lossy")
            .map_err(|_| "Could not remove duplicate startup registration")?;
    }
    Ok(())
}
