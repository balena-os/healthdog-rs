extern crate exec;
extern crate getopts;
extern crate libc;
extern crate nix;
extern crate systemd;

use getopts::{Options, ParsingStyle};
use libc::pid_t;
use nix::sys::signal::kill;
use nix::unistd::{getpid, Pid};
use std::{env, process, thread, time};
use systemd::daemon;

fn print_usage(opts: &Options) {
    println!("{}", opts.usage(&opts.short_usage("healthdog")));
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    let mut opts = Options::new();

    opts.parsing_style(ParsingStyle::StopAtFirstFree)
        .optflagopt("p", "pid", "pid to send watchdog events for", "PID")
        .reqopt("c", "healthcheck", "Set healthcheck command", "COMMAND")
        .optflag("h", "help", "Print this help menu");

    let matches = match opts.parse(&args) {
        Ok(m) => m,
        Err(err) => {
            println!("{}\n", err);
            print_usage(&opts);
            return;
        }
    };

    if matches.opt_present("h") {
        print_usage(&opts);
        return;
    }

    let health_cmd = match matches.opt_str("healthcheck") {
        Some(s) => s,
        None => process::exit(1),
    };

    let interval = match env::var("WATCHDOG_USEC")
        .ok()
        .and_then(|val| val.parse::<u64>().ok())
    {
        Some(usec) => time::Duration::from_secs(usec / 2 / 1_000_000),
        None => {
            println!("Invalid value for WATCHDOG_USEC");
            process::exit(1);
        }
    };

    match matches.opt_str("pid") {
        Some(pid) => {
            let pid = match pid.parse::<pid_t>() {
                Ok(pid) => pid,
                Err(err) => {
                    println!("{}\n", err);
                    print_usage(&opts);
                    return;
                }
            };

            loop {
                if kill(Pid::from_raw(pid), None).is_err() {
                    println!("Parent process exited");
                    process::exit(1);
                }

                match process::Command::new(&health_cmd).status() {
                    Ok(status) => {
                        if status.success() {
                            let message = [("WATCHDOG", "1")];

                            if let Err(err) = daemon::pid_notify(pid, false, message.iter()) {
                                println!("{}", err);
                                process::exit(1);
                            };
                        }
                    }
                    Err(err) => {
                        println!("{}", err);
                        process::exit(1);
                    }
                }

                thread::sleep(interval);
            }
        }
        None => {
            let pid = getpid();

            // first we start the helper child process that will run the healthcheck
            let helper = process::Command::new("/proc/self/exe")
                .args(&["--healthcheck", &health_cmd])
                .args(&["--pid", pid.to_string().as_str()])
                .spawn();

            match helper {
                Ok(mut helper) => {
                    // Then we execve to the requested program
                    let err = exec::Command::new(&matches.free[0])
                        .args(&matches.free[1..])
                        .exec();

                    // We only reach this point if execve failed
                    println!("Error: {}", err);

                    helper.kill().unwrap_or(());
                    process::exit(1);
                }
                Err(err) => {
                    println!("Error: {}", err);
                    process::exit(1);
                }
            }
        }
    }
}
