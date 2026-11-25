//! The isolated browser: launched as a transient unit inside its own
//! namespace, and tracked and stopped by process. Everything in that
//! namespace is the browser, so no process name is matched.

use std::collections::HashMap;
use std::time::Duration;

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use vpn_common::names::BROWSER_UNIT;

use crate::netns::{self, BROWSER};
use crate::unit;

pub fn pids() -> Vec<i32> {
    netns::pids(&BROWSER)
}

pub async fn launch(conn: &zbus::Connection, argv: Vec<String>, env: HashMap<String, String>, user: unit::User) -> zbus::Result<()> {
    unit::launch(conn, BROWSER_UNIT, "Browser inside its VPN namespace", &BROWSER, argv, env, user).await
}

/// A process's parent, from `/proc/<pid>/stat` (the field after the `)` that
/// closes the command name, then the state).
fn parent(pid: i32) -> Option<i32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    parent_of(&stat)
}

fn parent_of(stat: &str) -> Option<i32> {
    stat.rsplit_once(')')?.1.split_whitespace().nth(1)?.parse().ok()
}

/// Asks the browser to quit: SIGTERM to its top processes only (the ones
/// whose parent is outside the namespace), which close their children
/// cleanly. Then waits up to `limit` and kills what's left.
pub async fn stop(conn: &zbus::Connection, limit: Duration) {
    unit::stop(conn, BROWSER_UNIT).await;
    let all = tokio::task::spawn_blocking(pids).await.unwrap_or_default();
    for p in all.iter().filter(|p| parent(**p).is_none_or(|pp| !all.contains(&pp))) {
        let _ = kill(Pid::from_raw(*p), Signal::SIGTERM);
    }
    let deadline = tokio::time::Instant::now() + limit;
    loop {
        let left = tokio::task::spawn_blocking(pids).await.unwrap_or_default();
        if left.is_empty() {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            tracing::warn!("the browser didn't quit in time; killing it");
            for p in left {
                let _ = kill(Pid::from_raw(p), Signal::SIGKILL);
            }
            return;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn parent_from_stat() {
        // The command name may hold spaces and brackets.
        assert_eq!(super::parent_of("4242 (Isolated Web Co) S 4100 4100 4100 0 -1 4194560"), Some(4100));
        assert_eq!(super::parent_of("7 (a) b) R 1 7 7"), Some(1));
        assert_eq!(super::parent_of("garbage"), None);
    }
}
