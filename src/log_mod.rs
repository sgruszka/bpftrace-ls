use once_cell::sync::OnceCell;
use std::fs::{File, OpenOptions};
use std::io::Write;

pub const PROTO: u32 = 1 << 0;
pub const DIAGN: u32 = 1 << 1;
pub const COMPL: u32 = 1 << 2;
pub const NOTIF: u32 = 1 << 3;
pub const HOVER: u32 = 1 << 4;
pub const BTFRE: u32 = 1 << 5;

pub const VERBOSE_DEBUG: u32 = 0;

#[macro_export()]
macro_rules! log_err {
    ($fmt:expr) => {
        let msg = format!($fmt);
        let prefix = format!("{} {}:", file!(), line!());
        let full_msg = format!("{:<20} {}", prefix, msg);
        log_mod::log_fn(&full_msg);
    };
    ($fmt:expr, $( $arg:tt )* ) => {
        {
        let msg = format!($fmt, $( $arg )* );
        let prefix = format!("{} {}:", file!(), line!());
        let full_msg = format!("{:<20} {}", prefix, msg);
        log_mod::log_fn(&full_msg);
        }
    };
}

#[macro_export()]
macro_rules! log_dbg {
    ($type:expr, $fmt:expr) => {
        let msg = format!($fmt);
        let prefix = format!("{} {}:", file!(), line!());
        let full_msg = format!("{:<20} {}", prefix, msg);
        log_mod::log_cond_fn($type, &full_msg);
    };
    ($type:expr, $fmt:expr, $( $arg:tt )* ) => {
        {
        let msg = format!($fmt, $( $arg )* );
        let prefix = format!("{} {}:", file!(), line!());
        let full_msg = format!("{:<20} {}", prefix, msg);
        log_mod::log_cond_fn($type, &full_msg);
        }
    };
}

#[macro_export()]
macro_rules! log_vdbg {
    ($type:expr, $fmt:expr) => {
        if VERBOSE_DEBUG != 0 {
            let msg = format!($fmt);
            let prefix = format!("{} {}:", file!(), line!());
            let full_msg = format!("{:<20} {}", prefix, msg);
            log_mod::log_cond_fn($type, &full_msg);
        }
    };

    ($type:expr, $fmt:expr, $( $arg:tt )* ) => {
        {
        if VERBOSE_DEBUG != 0 {
            let msg = format!($fmt, $( $arg )* );
            let prefix = format!("{} {}:", file!(), line!());
            let full_msg = format!("{:<20} {}", prefix, msg);
            log_mod::log_cond_fn($type, &full_msg);
            }
        }
    };
}

#[derive(Debug)]
pub struct Logger {
    name: String,
    mask: u32,
}

static LOGGER: OnceCell<Logger> = OnceCell::new();

// TODO: handle errors without expect()
pub fn create_logger(filename: &str) -> Result<(), std::io::Error> {
    LOGGER
        .set(Logger {
            name: filename.to_string(),
            mask: PROTO | COMPL, // HOVER | BTFRE,
        })
        .expect("Was already initalized");
    // Create / truncate the file every time we run
    let _f = File::create(filename)?;
    Ok(())
}

pub fn log_fn(txt: &str) {
    if let Some(logger) = LOGGER.get() {
        let mut f = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&logger.name)
            .expect("Need to open a file");
        let _r = f.write_all(txt.as_bytes());
        let _r = f.write_all("\n".as_bytes());
    }
}

pub fn log_cond_fn(this_mask: u32, txt: &str) {
    if let Some(logger) = LOGGER.get() {
        if (logger.mask & this_mask) == 0 {
            return;
        }
        let mut f = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&logger.name)
            .expect("Need to open a file");
        let _r = f.write_all(txt.as_bytes());
        let _r = f.write_all("\n".as_bytes());
    }
}
