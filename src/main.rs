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

struct Args {
    pid: Option<pid_t>,
    healthcheck: String,
    help: bool,
    free: Vec<String>,
    interval: Option<time::Duration>,
}

fn main() -> Result<()> {
    let mut opts = Options::new();

    opts.parsing_style(ParsingStyle::StopAtFirstFree)
        .optopt("p", "pid", "pid to send watchdog events for", "PID")
        .reqopt("c", "healthcheck", "Set healthcheck command", "COMMAND")
        .optflag("h", "help", "Print this help menu");

    let args = match populate_args(&opts) {
        Ok(args) => args,
        Err(err) => {
            print_usage(&opts);
            bail!(err);
        }
    };

    if args.help {
        print_usage(&opts);
        return Ok(());
    }

    if let Some(pid) = args.pid {
        let interval = if let Some(interval) = args.interval {
            interval
        } else {
            println!("WATCHDOG_USEC not set");
            println!("Exiting, will not pet the watchdog");
            return Ok(());
        };

        loop {
            if kill(Pid::from_raw(pid), None).is_err() {
                println!("Parent process exited");
                return Ok(());
            }

            let status = process::Command::new(&args.healthcheck).status()?;

            if status.success() {
                let message = [("WATCHDOG", "1")];
                daemon::pid_notify(pid, false, message.iter())?;
            }

            thread::sleep(interval);
        }
    } else {
        let pid = getpid();

        // First we start the helper child process that will run the
        // healthcheck
        let mut helper = process::Command::new("/proc/self/exe")
            .args(&["--healthcheck", &args.healthcheck])
            .args(&["--pid", pid.to_string().as_str()])
            .spawn()?;

        // Then we execve to the requested program
        let err = exec::Command::new(&args.free[0])
            .args(&args.free[1..])
            .exec();

        helper.kill().unwrap_or(());

        bail!(err)
    }
}

fn populate_args(opts: &Options) -> Result<Args> {
    let cli_args: Vec<String> = env::args().skip(1).collect();
    let matches = opts.parse(&cli_args)?;

    let pid = matches
        .opt_str("pid")
        .map(|pid| pid.parse::<pid_t>())
        .transpose()?;

    // Healthcheck is a required option, so it is safe to unwrap.
    let healthcheck = matches.opt_str("healthcheck").unwrap();

    let help = matches.opt_present("h");

    let free = matches.free;

    let interval = get_watchdog_interval()?;

    Ok(Args {
        pid,
        healthcheck,
        help,
        free,
        interval,
    })
}

fn print_usage(opts: &Options) {
    println!("{}", opts.usage(&opts.short_usage("healthdog")));
}

/// Returns the watchdog interval duration. Returns `Option::None` when we are
/// not supposed to pet the watchdog.
fn get_watchdog_interval() -> Result<Option<time::Duration>> {
    match env::var("WATCHDOG_USEC") {
        Ok(val) => match val.parse::<u64>().ok() {
            Some(usec) => Ok(Some(time::Duration::from_micros(usec / 2))),
            None => bail!("Invalid value for WATCHDOG_USEC: {}", val),
        },
        Err(err) => {
            if err == env::VarError::NotPresent {
                Ok(Option::None)
            } else {
                bail!("Error reading WATCHDOG_USEC: {}", err);
            }
        }
    }
}
