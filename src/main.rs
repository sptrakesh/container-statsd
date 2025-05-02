mod ilp;
mod stats;

use std::io::{BufRead, BufReader};
use std::process::Command;
use std::thread;
use chrono::{DateTime, DurationRound, TimeDelta, Utc};
use clap::Parser;
use clap_num::number_range;
use log::{info, debug};
use sd_notify::{notify, NotifyState};
use serde::{Deserialize};

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

#[derive(Parser, Debug)]
#[command(version, about, long_about = None, ignore_errors(true))]
struct Cli {
  /// The host name to add to the published data.  Generally the name of the host docker daemon is running on.
  #[arg(short = 'n', long = "node")]
  host: String,
  /// The mode to use when publishing to QuestDB.  Defaults to avg.
  #[arg(short, long, default_value_t, value_enum)]
  mode: Mode,
  /// The QuestDB host to publish to.  Defaults to localhost.
  #[arg(short, long, default_value = "localhost")]
  questdb: String,
  /// The series name to publish to.  Defaults to containerStats.
  #[arg(short, long, default_value = "containerStats")]
  table: String,
  /// The interval for which statistics are gathered.  Defaults to 5 minutes.  Must be between 1 and 15.
  #[arg(short, long, default_value_t = 5, value_parser=valid_interval)]
  interval: u8
}

impl Cli
{
  fn clone(&self) -> Cli
  {
    Cli{table: self.table.clone(), host: self.host.clone(), questdb: self.questdb.clone(), mode: self.mode, interval: self.interval}
  }
}

fn main()
{
  let args = Cli::parse();
  simple_logger::init_with_env().unwrap();
  if cfg!(target_os = "linux") { let _ = notify(true, &[NotifyState::Ready]); }

  let size = (args.interval as usize) * 60 * 32;
  let mut vec : Vec<Stats> = Vec::with_capacity(size);
  let mut published : DateTime<Utc> = Utc::now().duration_round_up(TimeDelta::try_minutes(args.interval as i64).unwrap()).unwrap();
  info!("Publishing stats at {:?} for {}", published, args.host);

  loop
  {
    let records = read();
    vec.extend(records);
    debug!("Gathered {:?} statistics for {}", vec.len(), args.host);
    if cfg!(target_os = "linux") { let _ = notify(true, &[NotifyState::Watchdog, NotifyState::Status(format!("Gathered statistics for {}", args.host).as_str())]); }

    if Utc::now() > published
    {
      if vec.len() > 0
      {
        let copy = args.clone();
        thread::spawn(move || {
          let data = gather(&copy, vec);
          publish(&copy, data, published).expect("Failed to publish stats");
        });

        published = Utc::now().duration_round_up(TimeDelta::try_minutes(args.interval as i64).unwrap()).unwrap();
        info!("Publishing stats at {:?} for {}", published, args.host);
        vec = Vec::with_capacity(size);
      }
    }
  }
}

fn read() -> Vec<Stats>
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