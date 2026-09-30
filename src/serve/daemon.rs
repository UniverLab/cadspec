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
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
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
