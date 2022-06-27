extern crate anyhow;
extern crate exec;
extern crate getopts;
extern crate libc;
extern crate nix;
extern crate systemd;

use anyhow::{bail, Result};
use getopts::{Options, ParsingStyle};
use libc::pid_t;
use nix::sys::signal::kill;
use nix::unistd::{getpid, Pid};
use std::{env, process, thread, time};
use systemd::daemon;

fn print_usage(opts: &Options) {
    println!("{}", opts.usage(&opts.short_usage("healthdog")));
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();

    let mut opts = Options::new();

    opts.parsing_style(ParsingStyle::StopAtFirstFree)
        .optopt("p", "pid", "pid to send watchdog events for", "PID")
        .reqopt("c", "healthcheck", "Set healthcheck command", "COMMAND")
        .optflag("h", "help", "Print this help menu");

    let matches = match opts.parse(&args) {
        Ok(m) => m,
        Err(err) => {
            print_usage(&opts);
            bail!(err)
        }
    };

    if matches.opt_present("h") {
        print_usage(&opts);
        return Ok(());
    }

    let health_cmd = matches.opt_str("healthcheck").unwrap();

    let interval = watchdog_interval()?;

    match matches.opt_str("pid") {
        Some(pid) => {
            let interval = match interval {
                None => {
                    println!("Exiting, will not pet the watchdog");
                    return Ok(());
                }
                Some(value) => value,
            };

            let pid = match pid.parse::<pid_t>() {
                Ok(pid) => pid,
                Err(err) => {
                    print_usage(&opts);
                    bail!(err)
                }
            };

            loop {
                if kill(Pid::from_raw(pid), None).is_err() {
                    println!("Parent process exited");
                    return Ok(());
                }

                let status = process::Command::new(&health_cmd).status()?;

                if status.success() {
                    let message = [("WATCHDOG", "1")];
                    daemon::pid_notify(pid, false, message.iter())?;
                }

                thread::sleep(interval);
            }
        }
        None => {
            let pid = getpid();

            // first we start the helper child process that will run the
            // healthcheck
            let mut helper = process::Command::new("/proc/self/exe")
                .args(&["--healthcheck", &health_cmd])
                .args(&["--pid", pid.to_string().as_str()])
                .spawn()?;

            // Then we execve to the requested program
            let err = exec::Command::new(&matches.free[0])
                .args(&matches.free[1..])
                .exec();

            helper.kill().unwrap_or(());

            bail!(err)
        }
    }
}

/// Returns the watchdog interval duration. Returns `Option::None` when we are
/// not supposed to pet the watchdog.
fn watchdog_interval() -> Result<Option<time::Duration>> {
    match env::var("WATCHDOG_USEC") {
        Ok(val) => match val.parse::<u64>().ok() {
            Some(usec) => Ok(Some(time::Duration::from_micros(usec / 2))),
            None => bail!("Invalid value for WATCHDOG_USEC: {}", val),
        },
        Err(err) => {
            if err == env::VarError::NotPresent {
                println!("WATCHDOG_USEC not set");
                Ok(Option::None)
            } else {
                bail!("Error reading WATCHDOG_USEC: {}", err);
            }
        }
    }
}
