//! JetKVM passwords in the desktop's Secret Service (GNOME Keyring, KWallet),
//! through `secret-tool` (libsecret; Debian/Ubuntu: `libsecret-tools`). The
//! core never stores them: it gets them in memory with `set_jetkvm_password`
//! before it opens a JetKVM (docs/JETKVM.md).

use std::io::Write;
use std::process::{Command, Stdio};

use kvms_core::profiles::HostProfile;

const SERVICE: &str = "MiSTerKVMS JetKVM";

/// The key a JetKVM is filed under: its address, as the core keys it.
pub fn key(address: &str) -> String {
    let a = address.trim();
    let a = a.strip_prefix("jetkvm://").or_else(|| a.strip_prefix("http://")).unwrap_or(a);
    a.trim_end_matches('/').to_ascii_lowercase()
}

pub fn load(address: &str) -> String {
    Command::new("secret-tool")
        .args(["lookup", "service", SERVICE, "address", &key(address)])
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim_end_matches('\n').to_string())
        .unwrap_or_default()
}

/// Save (or, empty, delete) the password and hand it to the core. Err: it
/// couldn't be kept (no `secret-tool`, or no keyring running); the core
/// still has it until the app quits.
pub fn save(address: &str, password: &str) -> Result<(), String> {
    let key = key(address);
    kvms_core::controller::set_jetkvm_password(key.clone(), password.to_string());
    if password.is_empty() {
        let _ = Command::new("secret-tool").args(["clear", "service", SERVICE, "address", &key]).status();
        return Ok(());
    }
    let label = format!("MiSTerKVMS: JetKVM {key}");
    let mut child = Command::new("secret-tool")
        .args(["store", "--label", &label, "service", SERVICE, "address", &key])
        .stdin(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "secret-tool isn't installed (Debian/Ubuntu: sudo apt install libsecret-tools)".to_string())?;
    // The password goes on stdin, never on the command line.
    child.stdin.take().ok_or("secret-tool: no stdin")?.write_all(password.as_bytes()).map_err(|e| e.to_string())?;
    match child.wait() {
        Ok(s) if s.success() => Ok(()),
        _ => Err("the desktop keyring didn't take it (is GNOME Keyring or KWallet running?)".into()),
    }
}

/// The JetKVM a host's video comes from directly, if any.
pub fn jetkvm_of(profile: &HostProfile) -> Option<String> {
    profile.jetkvm_address().map(|a| key(&a))
}

/// At start-up and when hosts change: the core gets every JetKVM password
/// the hosts need.
pub fn hand_to(profiles: &[HostProfile]) {
    let mut done = std::collections::HashSet::new();
    for address in profiles.iter().filter_map(jetkvm_of) {
        if done.insert(address.clone()) {
            kvms_core::controller::set_jetkvm_password(address.clone(), load(&address));
        }
    }
}
