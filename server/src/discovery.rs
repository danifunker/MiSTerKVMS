//! mDNS: each stream is announced on its own (`<stream>._mkvm._tcp.local.`),
//! so viewers list MiSTers, not servers (docs/CAPTURE-SERVER.md §5.1).

use std::time::Duration;

use mdns_sd::{DaemonEvent, ServiceDaemon, ServiceInfo};

use crate::log;

pub struct Discovery {
    daemon: ServiceDaemon,
    /// What's announced: (stream, MiSTer address) and its mDNS full name.
    registered: Vec<((String, String), String)>,
    port: u16,
}

impl Discovery {
    pub fn start() -> Result<Self, String> {
        let daemon = ServiceDaemon::new().map_err(|e| e.to_string())?;
        // A rename means viewers see a stale "<stream> (2)" that never
        // resolves: worth a line in the log.
        if let Ok(events) = daemon.monitor() {
            std::thread::spawn(move || {
                while let Ok(event) = events.recv() {
                    if let DaemonEvent::NameChange(change) = event {
                        log(&format!("mDNS: '{}' renamed to '{}' on {}", change.original, change.new_name, change.intf_name));
                    }
                }
            });
        }
        Ok(Self { daemon, registered: Vec::new(), port: 0 })
    }

    /// Announce exactly these streams: (name, MiSTer address). Streams that
    /// didn't change are left alone. A changed one is withdrawn and, after a
    /// pause, announced again: re-announcing it at once, the daemon hears
    /// its own goodbye (with the old TXT) on another network adapter, takes
    /// it for someone else's, and renames the stream "<stream> (2)" with no
    /// address record, a dead entry viewers keep for an hour.
    pub fn announce(&mut self, streams: &[(String, String)], port: u16) {
        if port != self.port {
            self.port = port;
            self.withdraw(|_| true);
        }
        let withdrawn = self.withdraw(|s| !streams.contains(s));
        let new: Vec<&(String, String)> = streams.iter().filter(|s| !self.registered.iter().any(|(r, _)| r == *s)).collect();
        if new.is_empty() {
            return;
        }
        if new.iter().any(|(name, _)| withdrawn.contains(name)) {
            std::thread::sleep(Duration::from_millis(1500));
        }
        let host = hostname();
        for stream in new {
            let (name, mister) = stream;
            let props = [
                ("stream", name.as_str()),
                ("mister", mister.as_str()),
                ("protocol", "1"),
                ("server", host.as_str()),
            ];
            // Our own host record, not the computer's: the OS's responder
            // (mDNSResponder, Windows' mDNS) owns `<host>.local` and renames
            // the computer when someone else claims it. Viewers use the
            // addresses, not this name.
            let info = ServiceInfo::new(mkvm::SERVICE_TYPE, &instance_name(name), &format!("{host}-mkvm.local."), "", port, &props[..])
                .map(|i| i.enable_addr_auto());
            match info {
                Ok(info) => {
                    let full = info.get_fullname().to_string();
                    match self.daemon.register(info) {
                        Ok(()) => self.registered.push((stream.clone(), full)),
                        Err(e) => log(&format!("mDNS: can't announce '{name}': {e}")),
                    }
                }
                Err(e) => log(&format!("mDNS: '{name}': {e}")),
            }
        }
        log(&format!("mDNS: announcing {} stream(s) as {}", self.registered.len(), mkvm::SERVICE_TYPE));
    }

    /// Withdraws the announced streams `which` picks, waiting for their
    /// goodbyes to go out. Returns their names.
    fn withdraw(&mut self, which: impl Fn(&(String, String)) -> bool) -> Vec<String> {
        let mut names = Vec::new();
        self.registered.retain(|(stream, full)| {
            if !which(stream) {
                return true;
            }
            if let Ok(done) = self.daemon.unregister(full) {
                let _ = done.recv_timeout(Duration::from_secs(2));
            }
            names.push(stream.0.clone());
            false
        });
        names
    }
}

/// mDNS instance names can't contain dots; they're shown as is otherwise.
fn instance_name(stream: &str) -> String {
    stream.replace('.', "-")
}

fn hostname() -> String {
    let name = std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_default();
    let name = if name.trim().is_empty() {
        std::process::Command::new("hostname")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_else(|| "misterkvms-capture".into())
    } else {
        name
    };
    name.trim().trim_end_matches(".local").to_lowercase().replace(' ', "-")
}
