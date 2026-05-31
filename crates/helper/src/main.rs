use anyhow::{Context, Result};
use postora_planner::{commands_for_request, detect_system, ActionId, ApplyRequest, CommandSpec};
use serde::Serialize;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;

#[derive(Debug, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum HelperEvent<'a> {
    Started { plan_id: &'a uuid::Uuid },
    Command { action: ActionId, command: String },
    Progress { message: String },
    Success { message: String },
    Failure { message: String },
}

fn main() {
    if let Err(error) = run() {
        let _ = emit(&HelperEvent::Failure {
            message: format!("{error:#}"),
        });
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .context("failed to read helper JSON request from stdin")?;
    let request: ApplyRequest = serde_json::from_str(&input).context("malformed helper JSON request")?;
    emit(&HelperEvent::Started {
        plan_id: &request.plan_id,
    })?;

    let info = detect_system();
    let commands = commands_for_request(&request, &info)?;
    if commands.is_empty() {
        emit(&HelperEvent::Success {
            message: "All selected actions are already complete.".into(),
        })?;
        return Ok(());
    }

    for (action, command) in commands {
        emit(&HelperEvent::Command {
            action,
            command: command.display(),
        })?;
        run_command(&command).with_context(|| format!("command failed: {}", command.display()))?;
    }

    emit(&HelperEvent::Success {
        message: "Selected changes were applied.".into(),
    })?;
    Ok(())
}

fn run_command(command: &CommandSpec) -> Result<()> {
    let mut child = Command::new(&command.program)
        .args(&command.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to start {}", command.program))?;

    let stdout = child.stdout.take().context("failed to capture command stdout")?;
    let stderr = child.stderr.take().context("failed to capture command stderr")?;
    let (sender, receiver) = mpsc::channel::<(bool, String)>();

    let stdout_sender = sender.clone();
    thread::spawn(move || {
        let mut lines = BufReader::new(stdout).lines();
        while let Some(Ok(line)) = lines.next() {
            if stdout_sender.send((false, line)).is_err() {
                break;
            }
        }
    });
    thread::spawn(move || {
        let mut lines = BufReader::new(stderr).lines();
        while let Some(Ok(line)) = lines.next() {
            if sender.send((true, line)).is_err() {
                break;
            }
        }
    });

    let mut stderr_lines = Vec::new();
    for (is_stderr, line) in receiver {
        if !line.trim().is_empty() {
            if is_stderr {
                stderr_lines.push(line.clone());
            }
            emit(&HelperEvent::Progress {
                message: line,
            })?;
        }
    }

    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("exit status: {status}; stderr: {}", stderr_lines.join("\n"));
    }
}

fn emit(event: &HelperEvent<'_>) -> Result<()> {
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    serde_json::to_writer(&mut lock, event)?;
    lock.write_all(b"\n")?;
    lock.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use postora_planner::ApplyRequest;

    #[test]
    fn rejects_malformed_json() {
        let parsed: Result<ApplyRequest, _> = serde_json::from_str("{");
        assert!(parsed.is_err());
    }
}
