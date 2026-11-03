use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CoreState {
    Stopped,
    Running { pid: u32 },
    Stopping,
    Failed { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreLaunchRequest {
    pub binary_path: PathBuf,
    pub runtime_dir: PathBuf,
    pub config_path: PathBuf,
    /// Named pipe / unix socket for the controller; empty to rely on the
    /// config's `external-controller` instead.
    pub core_pipe_path: String,
}

impl CoreLaunchRequest {
    pub fn clash_args(&self) -> Vec<String> {
        let mut args = vec![
            "-d".to_owned(),
            self.runtime_dir.display().to_string(),
            "-f".to_owned(),
            config_arg(&self.runtime_dir, &self.config_path),
        ];
        if !self.core_pipe_path.is_empty() {
            args.extend(["--ext-ctl-pipe".to_owned(), self.core_pipe_path.clone()]);
        }
        args
    }

    pub fn test_config_args(&self) -> Vec<String> {
        vec![
            "-d".to_owned(),
            self.runtime_dir.display().to_string(),
            "-f".to_owned(),
            config_arg(&self.runtime_dir, &self.config_path),
            "--test-config".to_owned(),
        ]
    }

    pub fn validate_config(&self) -> anyhow::Result<()> {
        let mut command = Command::new(&self.binary_path);
        command.args(self.test_config_args());
        if let Some(binary_dir) = self.binary_path.parent() {
            command.current_dir(binary_dir);
        }
        hide_console_window(&mut command);

        let output = command.output()?;
        if output.status.success() {
            return Ok(());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "clash-rs config validation failed with {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            stdout.trim(),
            stderr.trim()
        )
    }
}

/// Bounded tail of the core's stdout/stderr, shared with the reader threads.
#[derive(Debug, Clone, Default)]
pub struct CoreLog {
    lines: Arc<Mutex<VecDeque<String>>>,
}

impl CoreLog {
    const CAPACITY: usize = 500;

    pub(crate) fn push(&self, line: String) {
        let mut lines = self.lines.lock().unwrap_or_else(|e| e.into_inner());
        if lines.len() == Self::CAPACITY {
            lines.pop_front();
        }
        lines.push_back(line);
    }

    pub fn clear(&self) {
        self.lines.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    pub fn tail(&self, count: usize) -> Vec<String> {
        let lines = self.lines.lock().unwrap_or_else(|e| e.into_inner());
        lines.iter().skip(lines.len().saturating_sub(count)).cloned().collect()
    }

    /// Message of the newest ERROR line among the last 30, for user display.
    pub fn last_error(&self) -> Option<String> {
        last_error(&self.tail(30))
    }

    /// Whether the TUN device came up: `Some(Ok)` once it did, `Some(Err)`
    /// with the reason if it failed, `None` while undecided. clash-rs keeps
    /// running without TUN on failure, so this is the only signal.
    pub fn tun_status(&self) -> Option<Result<(), String>> {
        for line in self.tail(CoreLog::CAPACITY).iter().rev() {
            if line.contains("tun initialization") {
                return Some(Err(last_error(std::slice::from_ref(line)).unwrap_or_else(|| line.clone())));
            }
            if ["setting up routes for tun", "route setup for existing tun", "tun started with fd"]
                .iter()
                .any(|marker| line.contains(marker))
            {
                return Some(Ok(()));
            }
        }
        None
    }

    /// Wait for [`Self::tun_status`] to decide. The success marker is logged
    /// *before* routes are installed (which can still fail), so success must
    /// hold for a settle period. Undecided after `timeout` counts as success,
    /// so a changed log format can't block connecting.
    pub async fn wait_for_tun(&self, timeout: std::time::Duration) -> Result<(), String> {
        const SETTLE: std::time::Duration = std::time::Duration::from_millis(2000);
        let started = std::time::Instant::now();
        let mut ok_since = None;
        loop {
            match self.tun_status() {
                Some(Err(reason)) => return Err(reason),
                Some(Ok(())) => {
                    let since = *ok_since.get_or_insert_with(std::time::Instant::now);
                    if since.elapsed() >= SETTLE {
                        return Ok(());
                    }
                }
                None if started.elapsed() >= timeout => return Ok(()),
                None => {}
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
}

/// The message of the newest ERROR log line, without timestamp/level/module:
/// `26-11-02 15:46:29 ERROR clash_lib::app: bind failed: in use` → `bind failed: in use`.
pub fn last_error(lines: &[String]) -> Option<String> {
    let line = lines.iter().rev().find(|line| line.contains(" ERROR "))?;
    let rest = line.split_once(" ERROR ")?.1.trim();
    // Drop a leading `module::path:` or `span: file.rs:12:` prefix.
    let message = match rest.split_once(": ") {
        Some((prefix, message)) if !prefix.contains(' ') => message,
        _ => rest,
    };
    Some(message.trim().to_owned()).filter(|m| !m.is_empty())
}

fn pump_lines(source: impl Read + Send + 'static, log: CoreLog) {
    std::thread::spawn(move || {
        // clash-rs logs with ANSI colors; read bytes so a non-UTF-8 line
        // can't end the pump, and strip escapes before storing.
        let mut reader = BufReader::new(source);
        let mut buf = Vec::new();
        while matches!(reader.read_until(b'\n', &mut buf), Ok(n) if n > 0) {
            let line = strip_ansi(&String::from_utf8_lossy(&buf));
            let line = line.trim_end();
            if !line.is_empty() {
                log.push(line.to_owned());
            }
            buf.clear();
        }
    });
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            // CSI sequence: ESC [ ... final byte in @..~
            if chars.next() == Some('[') {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

#[derive(Debug)]
pub struct ClashProcessManager {
    state: CoreState,
    child: Option<Child>,
    /// Kill-on-close job the child is assigned to; dropping it (including when
    /// this process dies) takes the core down, so it can never be orphaned.
    job: Option<job::KillOnCloseJob>,
    log: CoreLog,
}

impl Default for ClashProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ClashProcessManager {
    pub fn new() -> Self {
        Self {
            state: CoreState::Stopped,
            child: None,
            job: None,
            log: CoreLog::default(),
        }
    }

    pub fn log(&self) -> &CoreLog {
        &self.log
    }

    pub fn state(&mut self) -> CoreState {
        self.refresh_state();
        self.state.clone()
    }

    pub fn start_direct(&mut self, request: &CoreLaunchRequest) -> anyhow::Result<CoreState> {
        self.refresh_state();
        if self.child.is_some() {
            anyhow::bail!("clash-rs core is already running");
        }


        let mut command = Command::new(&request.binary_path);
        command.args(request.clash_args());
        if let Some(binary_dir) = request.binary_path.parent() {
            command.current_dir(binary_dir);
        }
        hide_console_window(&mut command);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        job::prepare(&mut command);

        let mut child = command.spawn()?;
        let job = match job::KillOnCloseJob::assign(&child) {
            Ok(job) => job,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error.context("failed to bind clash-rs to a job object"));
            }
        };

        // Always drain both pipes: an undrained pipe fills up and blocks the
        // core on its next log write.
        self.log.clear();
        if let Some(stdout) = child.stdout.take() {
            pump_lines(stdout, self.log.clone());
        }
        if let Some(stderr) = child.stderr.take() {
            pump_lines(stderr, self.log.clone());
        }

        let pid = child.id();
        self.child = Some(child);
        self.job = Some(job);
        self.state = CoreState::Running { pid };
        Ok(self.state.clone())
    }

    pub fn stop(&mut self) -> anyhow::Result<CoreState> {
        self.refresh_state();
        let Some(mut child) = self.child.take() else {
            self.state = CoreState::Stopped;
            return Ok(self.state.clone());
        };

        self.state = CoreState::Stopping;
        if child.try_wait()?.is_none() {
            child.kill()?;
        }
        child.wait()?;
        self.job = None;
        self.state = CoreState::Stopped;
        Ok(self.state.clone())
    }

    fn refresh_state(&mut self) {
        let Some(child) = &mut self.child else {
            if !matches!(
                self.state,
                CoreState::Failed { .. }
            ) {
                self.state = CoreState::Stopped;
            }
            return;
        };

        match child.try_wait() {
            Ok(Some(status)) => {
                self.child = None;
                self.job = None;
                self.state = if status.success() {
                    CoreState::Stopped
                } else {
                    CoreState::Failed {
                        message: format!("clash-rs exited with {status}"),
                    }
                };
            }
            Ok(None) => {}
            Err(error) => {
                self.child = None;
                self.state = CoreState::Failed {
                    message: format!("failed to query clash-rs process: {error}"),
                };
            }
        }
    }
}

impl Drop for ClashProcessManager {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            if matches!(child.try_wait(), Ok(None)) {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
    }
}

#[cfg(windows)]
mod job {
    use std::{os::windows::io::AsRawHandle, process::Child};

    use windows::Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        },
    };

    pub fn prepare(_command: &mut std::process::Command) {}

    #[derive(Debug)]
    pub struct KillOnCloseJob(HANDLE);

    // The job handle is only used for CloseHandle; safe to move across threads.
    unsafe impl Send for KillOnCloseJob {}
    unsafe impl Sync for KillOnCloseJob {}

    impl KillOnCloseJob {
        pub fn assign(child: &Child) -> anyhow::Result<Self> {
            // SAFETY: plain Win32 calls on handles we own; `info` outlives the
            // SetInformationJobObject call, and the child handle stays valid
            // while `child` is borrowed.
            unsafe {
                let job = Self(CreateJobObjectW(None, None)?);
                let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                SetInformationJobObject(
                    job.0,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const std::ffi::c_void,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )?;
                AssignProcessToJobObject(job.0, HANDLE(child.as_raw_handle()))?;
                Ok(job)
            }
        }
    }

    impl Drop for KillOnCloseJob {
        fn drop(&mut self) {
            // SAFETY: handle came from CreateJobObjectW and is closed once.
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

#[cfg(not(windows))]
mod job {
    use std::process::{Child, Command};

    /// Linux: ask the kernel to SIGKILL the core when its parent dies — the
    /// equivalent of a kill-on-close job object.
    #[cfg(target_os = "linux")]
    pub fn prepare(command: &mut Command) {
        use std::os::unix::process::CommandExt;
        // SAFETY: prctl is async-signal-safe; runs between fork and exec.
        unsafe {
            command.pre_exec(|| {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    #[cfg(not(target_os = "linux"))]
    pub fn prepare(_command: &mut Command) {}

    #[derive(Debug)]
    pub struct KillOnCloseJob;

    impl KillOnCloseJob {
        pub fn assign(_child: &Child) -> anyhow::Result<Self> {
            Ok(Self)
        }
    }
}

fn config_arg(runtime_dir: &Path, config_path: &Path) -> String {
    if let Ok(relative) = config_path.strip_prefix(runtime_dir)
        && !relative.as_os_str().is_empty()
    {
        return relative.display().to_string();
    }

    config_path.display().to_string()
}

#[cfg(windows)]
fn hide_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console_window(_command: &mut Command) {}

#[cfg(all(test, windows))]
mod tests {
    use std::path::PathBuf;

    use super::CoreLaunchRequest;

    #[cfg(windows)]
    #[test]
    fn launch_request_uses_relative_config_inside_runtime_dir() {
        let request = CoreLaunchRequest {
            binary_path: PathBuf::from(r"C:\ProgramData\clash-rs-desktop\core\clash-rs.exe"),
            runtime_dir: PathBuf::from(r"C:\ProgramData\clash-rs-desktop\runtime"),
            config_path: PathBuf::from(r"C:\ProgramData\clash-rs-desktop\runtime\generated.yaml"),
            core_pipe_path: r"\\.\pipe\clash-rs-desktop-core".to_owned(),
        };

        assert_eq!(
            request.clash_args(),
            vec![
                "-d",
                r"C:\ProgramData\clash-rs-desktop\runtime",
                "-f",
                "generated.yaml",
                "--ext-ctl-pipe",
                r"\\.\pipe\clash-rs-desktop-core",
            ]
        );

        assert_eq!(
            request.test_config_args(),
            vec![
                "-d",
                r"C:\ProgramData\clash-rs-desktop\runtime",
                "-f",
                "generated.yaml",
                "--test-config",
            ]
        );
    }
}

#[cfg(test)]
mod log_tests {
    use super::CoreLog;

    fn log(lines: &[&str]) -> CoreLog {
        let log = CoreLog::default();
        for line in lines {
            log.push((*line).to_owned());
        }
        log
    }

    #[test]
    fn tun_status_reads_clash_rs_markers() {
        assert_eq!(log(&["x INFO clash: starting"]).tun_status(), None);
        assert_eq!(
            log(&["x INFO runner.rs:273: setting up routes for tun utun1989"]).tun_status(),
            Some(Ok(()))
        );
        assert_eq!(
            log(&["26-11-02 16:20:22 ERROR clash-lib/src/proxy/tun/runner.rs:325: tun initialization failed: permission denied."])
                .tun_status(),
            Some(Err("tun initialization failed: permission denied.".to_owned()))
        );
    }
}
