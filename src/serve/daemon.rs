//! Background daemon — detach, readiness wait, and stop control.
//!
// ── Background daemon ──────────────────────────────────────────────────────
//
// `serve` runs detached by default: a parent process validates the project and
// the port, spawns the real (foreground) server in its own process group with
// its output redirected to a log file, waits until the port actually accepts a
// connection, then prints the URL and exits. Waiting for real readiness means
// we never claim "running" for a server that failed to come up.

use super::inspect::open_browser;
use crate::parser::parse_project;
use anyhow::{bail, Context, Result};
use std::fs::{self, File};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn runtime_dir(project_dir: &Path) -> PathBuf {
    project_dir.join(".cadspec")
}

fn pid_path(project_dir: &Path) -> PathBuf {
    runtime_dir(project_dir).join("serve.pid")
}

fn log_path(project_dir: &Path) -> PathBuf {
    runtime_dir(project_dir).join("serve.log")
}

/// True if `pid` refers to a live process (`kill -0`).
fn process_alive(pid: u32) -> bool {
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// The recorded daemon pid for this project, but only if it is still alive.
fn running_pid(project_dir: &Path) -> Option<u32> {
    let pid: u32 = fs::read_to_string(pid_path(project_dir))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    process_alive(pid).then_some(pid)
}

/// Block until the server accepts a connection on `port`, or `timeout` elapses.
fn wait_until_ready(port: u16, timeout: Duration) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    wait_until_deadline(
        timeout,
        Instant::now,
        || TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok(),
        || std::thread::sleep(Duration::from_millis(100)),
    )
}

/// The readiness wait with its clock, its probe and its sleep injected, so the
/// deadline edge can be tested without racing a real timer: the budget is
/// spent only once the clock has advanced the whole `timeout` past its first
/// reading.
fn wait_until_deadline(
    timeout: Duration,
    now: impl Fn() -> Instant,
    ready: impl Fn() -> bool,
    sleep: impl Fn(),
) -> bool {
    let start = now();
    while now().saturating_duration_since(start) < timeout {
        if ready() {
            return true;
        }
        sleep();
    }
    false
}

/// Start the live preview server detached in the background (the default).
pub fn serve_daemon(project_dir: &Path, port: u16, open: bool) -> Result<()> {
    // Validate the project up front so config errors surface here, not in a log.
    parse_project(&project_dir.join("project.toml"))?;
    let project_dir = project_dir
        .canonicalize()
        .unwrap_or_else(|_| project_dir.to_path_buf());
    let url = format!("http://127.0.0.1:{}", port);

    if let Some(pid) = running_pid(&project_dir) {
        println!("◉ cadspec serve already running (pid {pid})");
        println!("  Preview: {url}");
        println!("  Stop with: cadspec serve --stop");
        if open {
            open_browser(&url);
        }
        return Ok(());
    }

    // Fail fast on a busy port instead of letting the detached child die quietly.
    match TcpListener::bind(("127.0.0.1", port)) {
        Ok(listener) => drop(listener),
        Err(e) => bail!("Cannot bind 127.0.0.1:{port} (port in use?): {e}"),
    }

    fs::create_dir_all(runtime_dir(&project_dir))?;
    let log = log_path(&project_dir);
    let log_file = File::create(&log)?;

    let exe = std::env::current_exe().context("cannot locate cadspec executable")?;
    let mut cmd = Command::new(exe);
    cmd.arg("serve")
        .arg("--foreground")
        .arg("--path")
        .arg(&project_dir)
        .arg("--port")
        .arg(port.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file.try_clone()?))
        .stderr(Stdio::from(log_file));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Own process group: survives the parent shell / agent command exiting.
        cmd.process_group(0);
    }
    let child = cmd.spawn().context("failed to spawn background server")?;
    let pid = child.id();
    fs::write(pid_path(&project_dir), pid.to_string())?;

    if wait_until_ready(port, Duration::from_secs(5)) {
        println!("◉ cadspec serve — running in background (pid {pid})");
        println!("  Preview: {url}");
        println!("  Logs:    {}", log.display());
        println!("  Stop with: cadspec serve --stop");
        if open {
            open_browser(&url);
        }
        Ok(())
    } else {
        let _ = fs::remove_file(pid_path(&project_dir));
        let tail = fs::read_to_string(&log).unwrap_or_default();
        bail!(
            "server did not come up within 5s. Log:\n{}",
            tail.trim_end()
        );
    }
}

/// Stop the background server running for this project.
pub fn serve_stop(project_dir: &Path, _port: u16) -> Result<()> {
    let project_dir = project_dir
        .canonicalize()
        .unwrap_or_else(|_| project_dir.to_path_buf());
    let pid_file = pid_path(&project_dir);

    let Some(pid) = running_pid(&project_dir) else {
        let _ = fs::remove_file(&pid_file); // clean up any stale pidfile
        println!("No cadspec serve daemon running for this project.");
        return Ok(());
    };

    let stopped = Command::new("kill")
        .arg(pid.to_string())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    let _ = fs::remove_file(&pid_file);

    if stopped {
        println!("✓ Stopped cadspec serve (pid {pid}).");
        Ok(())
    } else {
        bail!("failed to stop process {pid}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A clock the test drives by hand, so a deadline edge can be reached
    /// exactly instead of racing a real timer.
    struct FakeClock(Cell<Instant>);

    impl FakeClock {
        fn new() -> Self {
            FakeClock(Cell::new(Instant::now()))
        }

        fn now(&self) -> Instant {
            self.0.get()
        }

        fn advance(&self, by: Duration) -> Instant {
            let next = self.0.get() + by;
            self.0.set(next);
            next
        }
    }

    /// Unique temp dir per test; removed on drop even when an assert panics.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(prefix: &str) -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "{prefix}_{}_{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::SeqCst)
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn runtime_and_log_paths_follow_the_project_dir() {
        let base = Path::new("/x");
        assert_eq!(runtime_dir(base), PathBuf::from("/x/.cadspec"));
        assert_eq!(log_path(base), PathBuf::from("/x/.cadspec/serve.log"));
    }

    #[test]
    fn process_alive_accepts_the_current_process() {
        assert!(process_alive(std::process::id()));
    }

    #[test]
    fn process_alive_rejects_a_pid_that_does_not_exist() {
        // A `true` stub would pass the live-pid assert above; a dead pid
        // must report false so the stub fails here.
        assert!(!process_alive(1_000_000_000));
    }

    #[test]
    fn serve_stop_clears_a_stale_pid_file() {
        // A dead pid in the file means no daemon: stop must remove the
        // stale file and succeed. An `Ok(())` stub would leave the file.
        let dir = TempDir::new("cadspec_daemon_stale");
        fs::create_dir_all(runtime_dir(dir.path())).unwrap();
        fs::write(pid_path(dir.path()), "1000000000\n").unwrap();
        assert!(!process_alive(1_000_000_000), "stale pid must be dead");
        serve_stop(dir.path(), 0).expect("stale stop must succeed");
        assert!(
            !pid_path(dir.path()).exists(),
            "stale pid file must be removed"
        );
    }

    #[test]
    fn running_pid_needs_a_pid_file_for_a_live_process() {
        let dir = TempDir::new("cadspec_daemon_pid");
        // No pid file yet: nothing is running.
        assert_eq!(running_pid(dir.path()), None);

        // Our own pid in the file: reported as running.
        fs::create_dir_all(runtime_dir(dir.path())).unwrap();
        fs::write(pid_path(dir.path()), format!("{}\n", std::process::id())).unwrap();
        assert_eq!(running_pid(dir.path()), Some(std::process::id()));
    }

    #[test]
    fn wait_until_ready_requires_a_live_listener() {
        // Listening port: ready promptly, well inside the 1s budget.
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(wait_until_ready(port, Duration::from_secs(1)));
        drop(listener);

        // Same port with zero budget: the wait loop never runs, so this is
        // false without attempting any connection (no other test's listener
        // can make it flaky). It pins the `< deadline` guard: a `==`/`>`
        // rewrite still returns false here, but the live-listener assert
        // above then fails.
        assert!(!wait_until_ready(port, Duration::ZERO));
    }

    #[test]
    fn the_readiness_wait_stops_at_the_deadline_edge() {
        // The clock lands exactly on the deadline at the first guard, so the
        // budget is already spent: no probe may run. A `<=` rewrite would
        // probe once and answer `true` here.
        let clock = FakeClock::new();
        let probes = Cell::new(0);
        let ready = wait_until_deadline(
            Duration::ZERO,
            || clock.now(),
            || {
                probes.set(probes.get() + 1);
                true
            },
            || (),
        );
        assert!(!ready, "a spent deadline must not probe at all");
        assert_eq!(probes.get(), 0);
    }

    #[test]
    fn the_readiness_wait_probes_until_ready_and_gives_up_at_the_deadline() {
        // Answering on the third probe: ready, after exactly two failed tries.
        let clock = FakeClock::new();
        let probes = Cell::new(0);
        let ready = wait_until_deadline(
            Duration::from_secs(5),
            || clock.now(),
            || {
                probes.set(probes.get() + 1);
                probes.get() == 3
            },
            || (),
        );
        assert!(ready);
        assert_eq!(probes.get(), 3);

        // Never answering, with the clock moving past the deadline: bounded,
        // so it stops instead of looping forever.
        let clock = FakeClock::new();
        let probes = Cell::new(0);
        let ready = wait_until_deadline(
            Duration::from_millis(250),
            || clock.advance(Duration::from_millis(100)),
            || {
                probes.set(probes.get() + 1);
                false
            },
            || (),
        );
        assert!(!ready);
        assert_eq!(probes.get(), 2, "probes stop once the deadline passes");
    }

    #[test]
    fn serve_daemon_fails_fast_without_project_toml() {
        // No project.toml in here: parse_project errors before anything is
        // bound, spawned, or written — the error path only.
        let missing = TempDir::new("cadspec_daemon_missing");
        let err = serve_daemon(missing.path(), 0, false)
            .expect_err("a directory without project.toml must not start a daemon");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("project.toml"),
            "error should mention project.toml: {msg}"
        );
    }
}
