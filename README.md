# container-statsd
Daemon process that continually executes `docker stats`, aggregates the
output and publishes to QuestDB periodically.  Stats are continually
polled to ensure that we can capture temporary spikes, short scheduled
job runs, etc.

## Command line arguments
The following arguments are supported for running the process:
* `-n|--node` The host name to add to the published data.  Generally the name
  of the host docker daemon is running on.
* `-m|--mode` The mode to use when publishing to QuestDB.  Defaults to `avg`.
* `-q|--questdb` The QuestDB host to publish to.  Defaults to `localhost`.
* `-t|--table` The series name to publish to.  Defaults to `containerStats`.
* `-i|--interval` The interval in minutes for which statistics are aggregated.
  Defaults to `5` minutes. Must be between `1` and `15`.
* `-w|--watchdog` *Linux only!*.  Enable or disable systemd watchdog notifications.
  If enabled, the systemd service unit **must** have `WatchdogSec` set.

## Run
Service will typically be run as a *service* through *systemd*.  The service
[unit](systemd/container-statsd.service) sample file can be used as a template
for setting up the service.

```shell
mkdir -p ~/.config/systemd/user/
cp <path to checked out source>/systemd/container-statsd.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable container-statsd.service
systemctl --user start container-statsd.service
```

To follow the service logs, use `systemctl` or `journalctl` as shown:
```shell
systemctl --user status container-statsd.service --full -n100
journalctl -f --user-unit container-statsd
```
