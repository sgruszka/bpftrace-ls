use std::env;
use std::io;
use std::process::Output;
use std::process::{Command, Stdio};
use std::sync::LazyLock;

fn test_sudo() -> bool {
    let res = Command::new("sudo")
        .arg("bpftrace")
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    if let Ok(status) = res {
        return status.success();
    }

    false
}

fn test_dry_run() -> bool {
    let args = ["--dry-run", "-e", r"BEGIN { exit() }"];

    if let Ok(output) = bpftrace_command(&args) {
        if output.status.success() {
            return true;
        }
    }

    false
}

struct UseSudo(bool);
struct UseDryRun(bool);
struct CustomCommand(Option<String>);

impl UseSudo {
    fn new() -> Self {
        UseSudo(test_sudo())
    }
}

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

static USE_SUDO: LazyLock<UseSudo> = LazyLock::new(UseSudo::new);
static USE_DRY_RUN: LazyLock<UseDryRun> = LazyLock::new(UseDryRun::new);
static CUSTOM_COMMAND: LazyLock<CustomCommand> = LazyLock::new(CustomCommand::new);

pub fn init() {
    LazyLock::force(&USE_DRY_RUN);
}

pub fn bpftrace_command(args: &[&str]) -> io::Result<Output> {
    if let Some(custom_cmd) = &CUSTOM_COMMAND.0 {
        return Command::new(custom_cmd).args(args).output();
    }

    let mut cmd = if USE_SUDO.0 {
        Command::new("sudo")
    } else {
        Command::new("bpftrace")
    };

    if USE_SUDO.0 {
        cmd.arg("bpftrace");
    }

    cmd.args(args).output()
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
