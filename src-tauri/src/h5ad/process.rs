use serde_json::Value;
use std::{
    io::{self, BufRead, BufReader, Read, Seek, Write},
    path::Path,
    process::{ChildStderr, ChildStdout, Command, Stdio},
    sync::mpsc,
    thread::{self, JoinHandle},
};

pub(super) const MAX_OUTPUT: usize = 256 * 1024;
type WorkerMessage = Result<Value, &'static str>;

pub(super) fn worker_command(
    python: &Path,
    script: &str,
    path: &Path,
    options: &Value,
    sha: &str,
    job: &Value,
) -> io::Result<Command> {
    // Linux caps individual exec arguments at 128 KiB on 4 KiB page
    // systems. Pass the embedded worker on a private, anonymous file instead.
    let mut input = tempfile::tempfile()?;
    input.write_all(script.as_bytes())?;
    input.rewind()?;
    let mut command = Command::new(python);
    command
        .args(["-I", "-B", "-u", "-"])
        .env_remove("PYTHONPATH")
        .env_remove("PYTHONHOME")
        .env_remove("VIRTUAL_ENV")
        .env_remove("PYTHONUSERBASE")
        .env_remove("PYTHONEXECUTABLE")
        .env_remove("__PYVENV_LAUNCHER__")
        .arg(path)
        .arg(options.to_string())
        .arg(sha)
        .arg(job.to_string())
        .stdin(Stdio::from(input))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    Ok(command)
}
pub(super) fn read_worker_pipes(
    stdout: ChildStdout,
    stderr: ChildStderr,
) -> (
    mpsc::Receiver<WorkerMessage>,
    JoinHandle<()>,
    JoinHandle<()>,
) {
    let (tx, rx) = mpsc::sync_channel(32);
    let reader = thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut total = 0;
        loop {
            let mut bytes = Vec::new();
            let read = reader
                .by_ref()
                .take((MAX_OUTPUT + 1) as u64)
                .read_until(b'\n', &mut bytes);
            match read {
                Ok(0) => break,
                Ok(n) => {
                    total += n;
                    if total > MAX_OUTPUT {
                        let _ = tx.send(Err("worker_output_too_large"));
                        break;
                    }
                    if tx
                        .send(
                            serde_json::from_slice::<Value>(&bytes)
                                .map_err(|_| "worker_protocol_error"),
                        )
                        .is_err()
                    {
                        break;
                    }
                }
                Err(_) => {
                    let _ = tx.send(Err("worker_protocol_error"));
                    break;
                }
            }
        }
    });
    // Drain stderr without retaining unbounded tracebacks or exposing file paths to UI.
    let errors = thread::spawn(move || {
        let _ = std::io::copy(&mut BufReader::new(stderr), &mut std::io::sink());
    });
    (rx, reader, errors)
}
