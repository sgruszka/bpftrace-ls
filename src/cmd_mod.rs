use std::env;
use std::io;
use std::process::Command;
use std::process::Output;
use std::sync::LazyLock;
use std::sync::OnceLock;

static USE_SUDO: OnceLock<bool> = OnceLock::new();
static USE_DRY_RUN: OnceLock<bool> = OnceLock::new();

struct CustomCommand(Option<String>);
impl CustomCommand {
    fn new() -> Self {
        CustomCommand(env::var("BPFTRACE_LS_COMMAND").ok())
    }
}
static CUSTOM_COMMAND: LazyLock<CustomCommand> = LazyLock::new(CustomCommand::new);

fn sudo_bpftrace_command(use_sudo: bool, args: &[&str]) -> io::Result<Output> {
    let mut cmd = if use_sudo {
        Command::new("sudo")
    } else {
        Command::new("bpftrace")
    };

    if use_sudo {
        cmd.arg("bpftrace");
    }

    cmd.args(args).output()
}

pub fn bpftrace_command(args: &[&str]) -> io::Result<Output> {
    if let Some(custom_cmd) = &CUSTOM_COMMAND.0 {
        return Command::new(custom_cmd).args(args).output();
    }

    if let Some(use_sudo) = USE_SUDO.get() {
        return sudo_bpftrace_command(*use_sudo, args);
    }

    if let Ok(output) = sudo_bpftrace_command(false, args) {
        if output.status.success() {
            let _ = USE_SUDO.set(false);
            return Ok(output);
        }
    }

    let _ = USE_SUDO.set(true);
    sudo_bpftrace_command(true, args)
}

pub fn bpftrace_dry_run_command(prog: &str) -> io::Result<Output> {
    let args_dry_run = vec!["--dry-run", "-e", prog];
    let args_d = vec!["-d", "-e", prog];

    if let Some(use_dry_run) = USE_DRY_RUN.get() {
        if *use_dry_run {
            return bpftrace_command(&args_dry_run);
        } else {
            return bpftrace_command(&args_d);
        }
    };

    if let Ok(output) = bpftrace_command(&args_dry_run) {
        if output.status.success() {
            let _ = USE_DRY_RUN.set(true);
        }
        return Ok(output);
    }

    let _ = USE_DRY_RUN.set(false);
    bpftrace_command(&args_d)
}
