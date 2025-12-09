use std::process::{Command, Stdio};
use std::sync::LazyLock;

fn test_sudo() -> bool {
    let res = Command::new("sudo")
        .arg("bpftrace")
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

fn test_debug(use_sudo: bool) -> bool {
    let mut cmd = if use_sudo {
        Command::new("sudo")
    } else {
        Command::new("bpftrace")
    };

    if use_sudo {
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

struct Config {
    use_sudo: bool,
    use_dbg_arg: bool,
}

impl Config {
    fn new() -> Self {
        let use_sudo = test_sudo();
        let use_dbg_arg = test_debug(use_sudo);

        Config {
            use_sudo,
            use_dbg_arg,
        }
    }
}

static CONFIG: LazyLock<Config> = LazyLock::new(|| Config::new());

pub fn init() {
    LazyLock::force(&CONFIG);
}

pub fn bpftrace_command() -> Command {
    /*
        if !CONFIG.env_var.is_empty() {

            Command::new(env_var)
                .args(args)
                .output()
        }
    */
    let mut cmd = if CONFIG.use_sudo {
        Command::new("sudo")
    } else {
        Command::new("bpftrace")
    };

    if CONFIG.use_sudo {
        cmd.arg("bpftrace");
    }

    cmd
}

pub fn bpftrace_debug_command() -> Command {
    let mut cmd = bpftrace_command();

    if CONFIG.use_dbg_arg {
        cmd.arg("-d");
        cmd.arg("all");
    } else {
        cmd.arg("-d");
    };

    cmd
}
