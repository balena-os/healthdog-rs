healthdog
=========

healthdog is a utility program that runs a healthcheck program periodically and
pets systemd's service watchdog.

Installing
----------

You can use `cargo build --release` to build this project and then copy
`./target/release/healthdog` to `/usr/local/bin/healthdog`.

Usage
-----

Let's say that we wish to run Docker and continuously monitor that the daemon
is responsive and restart in case it isn't.

First we create a program that will test the docker daemon and return **0** on
success, **1** otherwise.

* **`/usr/bin/check-docker`**:

```bash
#!/bin/sh

set -o errexit

# Check that info works
docker info > /dev/null
# Check that we can read containers from disk
docker ps > /dev/null
```

Then we prefix the `ExecStart` directive with healthdog and also set our desired `WatchdogSec` value.

* **`docker.service`**

```ini
[Unit]
Description=Docker Application Container Engine

[Service]
Type=simple
ExecStart=/usr/local/bin/healthdog --healthcheck=check-docker /usr/bin/dockerd
WatchdogSec=10
Restart=always

[Install]
WantedBy=multi-user.target
```

The service will spawn healthdog which in turn will run `check-docker` with a delay
of 5 seconds (half the systemd duration) between each run and pet the watchdog if it successfully
returns.

Note that the delay is initiated after the previous run has completed, not at set intervals.
This is to avoid parallel runs of the healthcheck if the command takes longer than expected.

For example, if the provided healthcheck command takes 60s and the systemd watchdog timeout is 90s,
it will never be successful as each run will take 45s delay + 60s runtime.
