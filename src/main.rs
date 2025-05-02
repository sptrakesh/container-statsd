mod ilp;
mod stats;

use std::io::{BufRead, BufReader};
use std::process::Command;
use std::thread;
use chrono::{DateTime, DurationRound, TimeDelta, Utc};
use clap::Parser;

use ilp::{gather, publish};
use stats::{RawStats, Stats, createStats};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
  #[arg(short = 'n', long = "node")]
  host: String,
  // Accepted values are `avg` and `max`
  #[arg(short, long, default_value = "avg")]
  mode: String,
  #[arg(short, long, default_value = "localhost")]
  questdb: String,
  #[arg(short, long, default_value = "containerStats")]
  table: String
}

impl Cli
{
  fn clone(&self) -> Cli
  {
    Cli{table: self.table.clone(), host: self.host.clone(), questdb: self.questdb.clone(), mode: self.mode.clone()}
  }
}

fn main()
{
  let args = Cli::parse();
  let mut vec : Vec<Stats> = Vec::with_capacity(8192);
  let mut published : DateTime<Utc> = Utc::now().duration_round_up(TimeDelta::try_minutes(5).unwrap()).unwrap();

  loop
  {
    let records = read();
    vec.extend(records);

    if Utc::now() > published
    {
      if vec.len() > 0
      {
        let copy = args.clone();
        thread::spawn(move || {
          let data = gather(&copy, vec);
          publish(&copy, data, published).expect("Failed to publish stats");
        });

        published = Utc::now().duration_round_up(TimeDelta::try_minutes(5).unwrap()).unwrap();
        vec = Vec::with_capacity(8192);
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
    //if cfg!(target_os = "macos") { println!("{:?}", stats); }
    vec.push(stats);
    line.clear();
  }

  return vec;
}