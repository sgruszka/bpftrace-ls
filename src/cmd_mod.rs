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

fn test_debug() -> bool {
    let mut cmd = if USE_SUDO.0 {
        Command::new("sudo")
    } else {
        Command::new("bpftrace")
    };

    if USE_SUDO.0 {
        cmd.arg("bpftrace");
    }

    let res = cmd
        .arg("-d all")
        .arg("-e")
        .arg(r"BEGIN { exit() }")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    if let Ok(status) = res {
        return status.success();
    }

    false
}

struct UseSudo(bool);
struct UseDbgArg(bool);

impl UseSudo {
    fn new() -> Self {
        UseSudo(test_sudo())
    }
}

impl UseDbgArg {
    fn new() -> Self {
        UseDbgArg(test_debug())
    }
}

static USE_SUDO: LazyLock<UseSudo> = LazyLock::new(|| UseSudo::new());
static USE_DBG_ARG: LazyLock<UseDbgArg> = LazyLock::new(|| UseDbgArg::new());

pub fn init() {
    LazyLock::force(&USE_DBG_ARG);
}

pub fn bpftrace_command() -> Command {
    /*
        if !CONFIG.env_var.is_empty() {

            Command::new(env_var)
                .args(args)
                .output()
        }
    */
    let mut cmd = if USE_SUDO.0 {
        Command::new("sudo")
    } else {
        Command::new("bpftrace")
    };

    if USE_SUDO.0 {
        cmd.arg("bpftrace");
    }

    cmd
}

pub fn bpftrace_debug_command() -> Command {
    let mut cmd = bpftrace_command();

    if USE_DBG_ARG.0 {
        cmd.arg("-d");
        cmd.arg("all");
    } else {
        cmd.arg("-d");
    };

    cmd
}
