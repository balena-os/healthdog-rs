extern crate libc;
extern crate exec;
extern crate getopts;
extern crate systemd;

use std::collections::HashMap;
use getopts::{Options, ParsingStyle};
use std::{env, process, thread, time};
use libc::pid_t;

use systemd::daemon;

fn print_usage(opts: &Options) {
    println!("{}", opts.usage(&opts.short_usage("healthdog")));
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    let mut opts = Options::new();

    opts.parsing_style(ParsingStyle::StopAtFirstFree)
        .optflagopt(
            "p",
            "pid",
            "Send watchdog events on behalf of specified pid",
            "PID",
        )
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

    let interval = match env::var("WATCHDOG_USEC").ok().and_then(
        |val| val.parse::<u64>().ok(),
    ) {
        Some(usec) => time::Duration::from_secs(usec / 2 / 1_000_0000),
        None => {
            println!("Invalid value for WATCHDOG_USEC");
            process::exit(1);
        }
    };

    match matches.opt_str("pid") {
        Some(pid) => {
            println!("Parsing target pid: {}", pid);
            let pid = match pid.parse::<pid_t>() {
                Ok(pid) => pid,
                Err(err) => {
                    println!("{}\n", err);
                    print_usage(&opts);
                    return;
                }
            };

            loop {
                thread::sleep(interval);

                match process::Command::new(&health_cmd).status() {
                    Ok(status) => {
                        if status.success() {
                            let mut message = HashMap::new();
                            message.insert("WATCHDOG", "1");

                            match daemon::pid_notify(pid, false, message) {
                                Ok(_) => println!("Success!"),
                                Err(err) => {
                                    println!("{}\n", err);
                                    process::exit(1);
                                }
                            };
                        }
                    }
                    Err(err) => {
                        println!("{}\n", err);
                        process::exit(1);
                    }
                }
            }
        }
        None => {
            let pid = unsafe { libc::getpid() };
            println!("[{}] Spawning program and healthcheck", pid);

            // first we start the helper child process that will run the healthcheck
            process::Command::new("/proc/self/exe")
                .args(
                    &[
                        "--healthcheck",
                        matches.opt_str("healthcheck").unwrap().as_str(),
                    ],
                )
                .args(&["--pid", pid.to_string().as_str()])
                .spawn()
                .expect("failed to execute child");

            // Then we execve to the requested program
            let err = exec::Command::new(&matches.free[0])
                .args(&matches.free[1..])
                .exec();
            println!("Error: {}", err);
            process::exit(1);
        }
    }
}
