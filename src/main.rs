mod ilp;
mod stats;

use std::io::{BufRead, BufReader, Error};
use std::process::Command;
use std::thread;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use std::thread::JoinHandle;
use chrono::{DateTime, DurationRound, TimeDelta, Utc};
use clap::Parser;
use clap_num::number_range;
use log::{info, debug};
use serde::{Deserialize};

#[cfg(target_os = "linux")]
extern crate libsystemd;
#[cfg(target_os = "linux")]
use libsystemd::daemon::{self, NotifyState};

use ilp::{gather, publish};
use stats::{RawStats, Stats, createStats};

fn valid_interval(s: &str) -> Result<u8, String> {
  number_range(s, 1, 15)
}

#[derive(
  clap::ValueEnum, Clone, Default, Debug, Deserialize, PartialEq, Eq, Copy
)]
#[serde(rename_all = "lowercase")]
enum Mode {
  /// Average the aggregated statistics when publishing to QuestDB
  #[default]
  Avg,
  /// Use the maximum value from aggregated statistics when publishing to QuestDB
  Max
}

#[cfg(target_os = "linux")]
#[derive(
  clap::ValueEnum, Clone, Default, Debug, Deserialize, PartialEq, Eq, Copy
)]
#[serde(rename_all = "lowercase")]
enum Watchdog {
  /// Enable watchdog timer. Requires WatchdogSec set in service unit file.
  #[default]
  Enabled,
  /// Disable watchdog timer.  Mainly intended when run as non-systemd service.
  Disabled
}

#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None, ignore_errors(true))]
struct Cli {
  /// Optional list of disk/block device names for which disk usage statistics are to be captured.
  #[clap(short = 'b', long = "block-device")]
  disks: Vec<String>,
  /// The series name to publish disk information to.  Defaults to diskStats.
  #[arg(short, long, default_value = "diskStats")]
  disk_table: String,
  /// The host name to add to the published data.  Generally the name of the host docker daemon is running on.
  #[arg(short = 'n', long = "node")]
  host: String,
  /// The QuestDB host to publish to.  Defaults to localhost.
  #[arg(short, long, default_value = "localhost")]
  questdb: String,
  /// The series name to publish to.  Defaults to containerStats.
  #[arg(short, long, default_value = "containerStats")]
  table: String,
  #[cfg(target_os = "linux")]
  /// Enable systemd watchdog notifications.  Enable only if run via systemd.
  #[arg(short, long, default_value_t, value_enum)]
  watchdog: Watchdog,
  /// The mode to use when publishing to QuestDB.  Defaults to avg.
  #[arg(short, long, default_value_t, value_enum)]
  mode: Mode,
  /// The interval in minutes for which statistics are gathered.  Defaults to 5 minutes.  Must be between 1 and 15.
  #[arg(short, long, default_value_t = 5, value_parser=valid_interval)]
  interval: u8
}

fn next_publish(interval: u8) -> DateTime<Utc>
{
  Utc::now().duration_round_up(TimeDelta::try_minutes(interval as i64).unwrap()).unwrap()
}

fn timeout() -> Duration
{
  #[cfg(target_os = "linux")]
  return daemon::watchdog_enabled(false).expect("watchdog timeout not set");
  
  #[cfg(target_os = "macos")]
  return Duration::from_secs(10);
}

#[cfg(target_os = "linux")]
fn notify_watchdog(notified: &mut DateTime<Utc>, timeout: Duration)
{
  let now = Utc::now();
  let diff = now - *notified;
  if diff.num_milliseconds() > (timeout.as_millis() / 2) as i64
  {
    let _sent = daemon::notify(false, &[NotifyState::Watchdog]).expect("notify failed");
    *notified = now;
    debug!("Sent watchdog notification");
  }
}

fn statistics() -> Vec<Stats>
{
  let output = Command::new("docker").arg("stats").arg("--no-stream").arg("--format=json").output().expect("failed to execute process");
  let mut reader = BufReader::new(output.stdout.as_slice());
  let mut vec : Vec<Stats> = Vec::with_capacity(32);
  let mut line = String::new();
  while reader.read_line(&mut line).unwrap() > 0
  {
    let raw : RawStats = serde_json::from_str(&line.trim()).expect("failed to parse json");
    let stats = createStats(&raw);
    vec.push(stats);
    line.clear();
  }

  vec
}

fn publish_stats(args: &Cli, term: &Arc<AtomicBool>, interval: Duration) -> Option<JoinHandle<()>>
{
  let mut handle : Option<JoinHandle<()>> = None;
  let published = next_publish(args.interval);
  info!("Publishing stats at {:?} for {} with watchdog interval {}", published, args.host, interval.as_secs());

  #[cfg(target_os = "linux")]
  let mut notified = Utc::now();

  let size = (args.interval as usize) * 60 * 32;
  let mut vec : Vec<Stats> = Vec::with_capacity(size);
  while !term.load(Ordering::Relaxed)
  {
    let records = statistics();
    vec.extend(records);
    debug!("Gathered {:?} statistics for {}", vec.len(), args.host);

    #[cfg(target_os = "linux")]
    if args.watchdog == Watchdog::Enabled { notify_watchdog(&mut notified, interval); }

    if Utc::now() > published
    {
      if vec.len() > 0
      {
        let copy = args.clone();
        handle = Some(thread::spawn(move ||
            {
              let data = gather(&copy, vec);
              publish(&copy, data, published).expect("Failed to publish stats");
            }));

        info!("Publishing stats at {:?} for {}", published, args.host);
        #[cfg(target_os = "linux")]
        if args.watchdog == Watchdog::Enabled { notify_watchdog(&mut notified, interval); }
        break;
      }
    }
  }
  
  handle
}

fn main() -> Result<(), Error>
{
  let term = Arc::new(AtomicBool::new(false));
  signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&term)).expect("Error setting SIGTERM handler");
  signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&term)).expect("Error setting SIGTERM handler");
  
  let args = Cli::parse();
  simple_logger::init_with_env().unwrap();
  
  #[cfg(target_os = "linux")]
  let duration =
      {
        if args.watchdog == Watchdog::Enabled { timeout() }
        else { Duration::from_secs(0) }
      };
  #[cfg(target_os = "macos")]
  let duration = timeout();
  
  #[cfg(target_os = "linux")]
  if args.watchdog == Watchdog::Enabled && !daemon::booted() {
    panic!("Not running systemd, early exit.");
  };

  #[cfg(target_os = "linux")]
  if args.watchdog == Watchdog::Enabled { let _ = daemon::notify(false, &[NotifyState::Ready]).expect("notify failed"); }

  let mut handle : Option<JoinHandle<()>> = None;
  
  while !term.load(Ordering::Relaxed)
  {
    handle = publish_stats(&args, &term, duration);
  }

  #[cfg(target_os = "linux")]
  if args.watchdog == Watchdog::Enabled { let _sent = daemon::notify(true, &[NotifyState::Stopping]).expect("notify failed"); }
  
  if handle.is_some() { handle.take().unwrap().join().unwrap(); }

  Ok(())
}