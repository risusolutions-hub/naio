//! Child process spawning, streaming I/O, and lifecycle management.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

#[derive(Default, Clone)]
pub struct SpawnOpts {
    pub cwd: Option<PathBuf>,
    pub env: HashMap<String, String>,
    pub stdin_pipe: bool,
    pub stdout_pipe: bool,
    pub stderr_pipe: bool,
}

pub struct ChildProcess {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: Option<ChildStdout>,
    stderr: Option<ChildStderr>,
    returncode: Option<i32>,
}

impl ChildProcess {
    pub fn spawn(program: &str, args: &[String], opts: &SpawnOpts) -> std::io::Result<Self> {
        let mut cmd = Command::new(program);
        cmd.args(args);
        if let Some(cwd) = &opts.cwd {
            cmd.current_dir(cwd);
        }
        for (k, v) in &opts.env {
            cmd.env(k, v);
        }
        cmd.stdin(if opts.stdin_pipe {
            Stdio::piped()
        } else {
            Stdio::null()
        });
        cmd.stdout(if opts.stdout_pipe {
            Stdio::piped()
        } else {
            Stdio::null()
        });
        cmd.stderr(if opts.stderr_pipe {
            Stdio::piped()
        } else {
            Stdio::null()
        });
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let mut child = cmd.spawn()?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        Ok(Self {
            child,
            stdin,
            stdout,
            stderr,
            returncode: None,
        })
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    pub fn poll(&mut self) -> Option<i32> {
        if let Some(code) = self.returncode {
            return Some(code);
        }
        match self.child.try_wait() {
            Ok(Some(status)) => {
                let code = status.code().unwrap_or(-1);
                self.returncode = Some(code);
                Some(code)
            }
            Ok(None) => None,
            Err(_) => Some(-1),
        }
    }

    pub fn wait(&mut self, timeout: Option<Duration>) -> std::io::Result<i32> {
        if let Some(code) = self.returncode {
            return Ok(code);
        }
        if let Some(limit) = timeout {
            let start = Instant::now();
            loop {
                if let Some(code) = self.poll() {
                    return Ok(code);
                }
                if start.elapsed() >= limit {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "process wait timed out",
                    ));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        } else {
            let status = self.child.wait()?;
            let code = status.code().unwrap_or(-1);
            self.returncode = Some(code);
            Ok(code)
        }
    }

    pub fn kill(&mut self) -> std::io::Result<()> {
        self.child.kill()?;
        let _ = self.child.wait();
        self.returncode = Some(-1);
        Ok(())
    }

    pub fn terminate(&mut self) -> std::io::Result<()> {
        self.kill()
    }

    pub fn stdin_write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        match self.stdin.as_mut() {
            Some(s) => s.write(data),
            None => Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "stdin not piped",
            )),
        }
    }

    pub fn stdin_close(&mut self) {
        self.stdin = None;
    }

    pub fn stdout_read(&mut self, max: usize) -> std::io::Result<Vec<u8>> {
        match self.stdout.as_mut() {
            Some(s) => {
                let mut buf = vec![0u8; max];
                let n = s.read(&mut buf)?;
                buf.truncate(n);
                Ok(buf)
            }
            None => Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "stdout not piped",
            )),
        }
    }

    pub fn stdout_read_all(&mut self) -> std::io::Result<Vec<u8>> {
        match self.stdout.as_mut() {
            Some(s) => {
                let mut buf = Vec::new();
                s.read_to_end(&mut buf)?;
                Ok(buf)
            }
            None => Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "stdout not piped",
            )),
        }
    }

    pub fn stderr_read(&mut self, max: usize) -> std::io::Result<Vec<u8>> {
        match self.stderr.as_mut() {
            Some(s) => {
                let mut buf = vec![0u8; max.min(1_048_576)];
                let n = s.read(&mut buf)?;
                buf.truncate(n);
                Ok(buf)
            }
            None => Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "stderr not piped",
            )),
        }
    }

    pub fn stderr_read_all(&mut self) -> std::io::Result<Vec<u8>> {
        match self.stderr.as_mut() {
            Some(s) => {
                let mut buf = Vec::new();
                s.read_to_end(&mut buf)?;
                Ok(buf)
            }
            None => Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "stderr not piped",
            )),
        }
    }

    pub fn communicate(
        &mut self,
        input: Option<&[u8]>,
        timeout: Option<Duration>,
    ) -> std::io::Result<(Vec<u8>, Vec<u8>, i32)> {
        if let Some(data) = input {
            let _ = self.stdin_write(data)?;
        }
        self.stdin_close();
        let code = self.wait(timeout)?;
        let stdout = self.stdout_read_all().unwrap_or_default();
        let stderr = self.stderr_read_all().unwrap_or_default();
        Ok((stdout, stderr, code))
    }

    pub fn is_running(&mut self) -> bool {
        self.poll().is_none()
    }
}

pub fn run_output(
    program: &str,
    args: &[String],
    opts: &SpawnOpts,
) -> std::io::Result<(Vec<u8>, Vec<u8>, ExitStatus)> {
    let mut spawn_opts = opts.clone();
    spawn_opts.stdin_pipe = false;
    spawn_opts.stdout_pipe = true;
    spawn_opts.stderr_pipe = true;
    let mut child = ChildProcess::spawn(program, args, &spawn_opts)?;
    let (out, err, _) = child.communicate(None, None)?;
    let status = child.child.wait()?;
    Ok((out, err, status))
}
