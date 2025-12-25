use std::env;
use std::io;
use std::process::Command;
use std::process::Output;
use std::sync::LazyLock;
use std::sync::OnceLock;

fn test_dry_run() -> bool {
    let args = ["--dry-run", "-e", r"BEGIN { exit() }"];

    if let Ok(output) = bpftrace_command(&args) {
        if output.status.success() {
            return true;
        }
    }

    false
}

struct UseDryRun(bool);
struct CustomCommand(Option<String>);

impl UseDryRun {
    fn new() -> Self {
        UseDryRun(test_dry_run())
    }
}

impl CustomCommand {
    fn new() -> Self {
        CustomCommand(env::var("BPFTRACE_LS_COMMAND").ok())
    }
}

static USE_SUDO: OnceLock<bool> = OnceLock::new();

static USE_DRY_RUN: LazyLock<UseDryRun> = LazyLock::new(UseDryRun::new);
static CUSTOM_COMMAND: LazyLock<CustomCommand> = LazyLock::new(CustomCommand::new);

pub fn init() {
    LazyLock::force(&USE_DRY_RUN);
}

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

pub fn bpftrace_debug_command(args: &[&str]) -> io::Result<Output> {
    let mut debug_args;

    if USE_DRY_RUN.0 {
        debug_args = vec!["--dry-run"];
    } else {
        debug_args = vec!["-d"];
    };

    debug_args.extend(args);

    bpftrace_command(&debug_args)
}
