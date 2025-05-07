use std::collections::HashMap;
use std::ffi::{OsString};
use sysinfo::Disks;
use chrono::{DateTime, Utc};
use float_ord::sort;
use log::info;
use questdb::{
  Result,
  ingress::{
    Buffer,
    ColumnName,
    Sender,
    TableName,
    TimestampNanos
  },
};

use super::{Cli, Mode};
use super::stats::{Measurement, Stats, IO};

pub fn gather(cli: &Cli, stats: Vec<Stats>) -> Vec<Stats>
{
  info!("Aggregating {:?} container statistics for {}.", stats.len(), cli.host);
  let mut cpu : HashMap<String, Vec<f64>> = HashMap::new();
  let mut mem : HashMap<String, Vec<f64>> = HashMap::new();
  let mut memper : HashMap<String, Vec<f64>> = HashMap::new();
  let mut bioin : HashMap<String, Vec<f64>> = HashMap::new();
  let mut bioout : HashMap<String, Vec<f64>> = HashMap::new();
  let mut netin : HashMap<String, Vec<f64>> = HashMap::new();
  let mut netout : HashMap<String, Vec<f64>> = HashMap::new();
  let mut pids : HashMap<String, Vec<u32>> = HashMap::new();

  for stat in &stats
  {
    if cpu.contains_key(&stat.name) { cpu.get_mut(&stat.name).unwrap().push(stat.cpuPercentage); }
    else { cpu.insert(stat.name.clone(), vec![stat.cpuPercentage]); }

    if mem.contains_key(&stat.name) { mem.get_mut(&stat.name).unwrap().push(stat.memoryUsage.value); }
    else { mem.insert(stat.name.clone(), vec![stat.memoryUsage.value]); }

    if memper.contains_key(&stat.name) { memper.get_mut(&stat.name).unwrap().push(stat.memoryPercentage); }
    else { memper.insert(stat.name.clone(), vec![stat.memoryPercentage]); }

    if bioin.contains_key(&stat.name) { bioin.get_mut(&stat.name).unwrap().push(stat.blockIO.incoming.value); }
    else { bioin.insert(stat.name.clone(), vec![stat.blockIO.incoming.value]); }

    if bioout.contains_key(&stat.name) { bioout.get_mut(&stat.name).unwrap().push(stat.blockIO.outgoing.value); }
    else { bioout.insert(stat.name.clone(), vec![stat.blockIO.outgoing.value]); }

    if netin.contains_key(&stat.name) { netin.get_mut(&stat.name).unwrap().push(stat.netIO.incoming.value); }
    else { netin.insert(stat.name.clone(), vec![stat.netIO.incoming.value]); }

    if netout.contains_key(&stat.name) { netout.get_mut(&stat.name).unwrap().push(stat.netIO.outgoing.value); }
    else { netout.insert(stat.name.clone(), vec![stat.netIO.outgoing.value]); }

    if pids.contains_key(&stat.name) { pids.get_mut(&stat.name).unwrap().push(stat.pids); }
    else { pids.insert(stat.name.clone(), vec![stat.pids]); }
  }

  let mut vec : Vec<Stats> = Vec::with_capacity(32);

  fn compute(mode: Mode, values: &mut Vec<f64>) -> f64
  {
    if mode == Mode::Avg { return values.iter().sum::<f64>() / (values.len() as f64); }
    sort(values);
    values[values.len() - 1]
  }

  fn compute_pids(mode: Mode, values: &Vec<u32>) -> u32
  {
    if mode == Mode::Avg { return values.iter().sum::<u32>() / (values.len() as u32); }
    *values.iter().max().unwrap()
  }

  for (name, values) in &mut cpu
  {
    let mut st = Stats::new();
    st.name = name.clone();
    st.id = stats[0].id.clone();
    st.container = stats[0].container.clone();
    st.totalMemory.value = stats[0].totalMemory.value;
    st.totalMemory.unit = stats[0].totalMemory.unit.clone();
    st.cpuPercentage = compute(cli.mode, values);
    vec.push(st);
  }

  for (name, values) in &mut mem
  {
    let stat = vec.iter_mut().find(|s| s.name == *name).unwrap();
    stat.memoryUsage.unit = stats[0].memoryUsage.unit.clone();
    stat.memoryUsage.value = compute(cli.mode, values);
  }

  for (name, values) in &mut memper
  {
    let stat = vec.iter_mut().find(|s| s.name == *name).unwrap();
    stat.memoryPercentage = compute(cli.mode, values);
  }

  for (name, values) in &mut bioin
  {
    let stat = vec.iter_mut().find(|s| s.name == *name).unwrap();
    stat.blockIO.incoming.unit = stats[0].blockIO.incoming.unit.clone();
    stat.blockIO.incoming.value = compute(cli.mode, values);
  }

  for (name, values) in &mut bioout
  {
    let stat = vec.iter_mut().find(|s| s.name == *name).unwrap();
    stat.blockIO.outgoing.unit = stats[0].blockIO.outgoing.unit.clone();
    stat.blockIO.outgoing.value = compute(cli.mode, values);
  }

  for (name, values) in &mut netin
  {
    let stat = vec.iter_mut().find(|s| s.name == *name).unwrap();
    stat.netIO.incoming.unit = stats[0].netIO.incoming.unit.clone();
    stat.netIO.incoming.value = compute(cli.mode, values);
  }

  for (name, values) in &mut netout
  {
    let stat = vec.iter_mut().find(|s| s.name == *name).unwrap();
    stat.netIO.outgoing.unit = stats[0].netIO.outgoing.unit.clone();
    stat.netIO.outgoing.value = compute(cli.mode, values);
  }

  for (name, values) in &pids
  {
    let stat = vec.iter_mut().find(|s| s.name == *name).unwrap();
    stat.pids = compute_pids(cli.mode, values);
  }

  vec
}

pub fn publish(cli: &Cli, stats: Vec<Stats>, time: DateTime<Utc>) -> Result<()>
{
  let uri = format!("{:?}::addr={}:{}", cli.protocol, cli.questdb, cli.port);
  info!("Publishing {:?} container statistics for {} to {}.", stats.len(), cli.host, uri);

  let mut sender = Sender::from_conf(uri)?;
  let mut buffer = Buffer::new();
  let table = TableName::new(cli.table.as_str())?;
  
  let chost = ColumnName::new("host")?;
  let ccontainer = ColumnName::new("container")?;
  let cname = ColumnName::new("name")?;
  let cid = ColumnName::new("id")?;
  let ccpu = ColumnName::new("cpu")?;
  let cmp = ColumnName::new("memory_percentage")?;
  let cpids = ColumnName::new("pids")?;
  
  fn add_io(buffer: &mut Buffer, io: &IO, prefix: &str)
  {
    fn bytes(measurement: &Measurement) -> f64
    {
      if measurement.unit == "KB" { return measurement.value * 1024.0; }
      else if measurement.unit == "MB" { return measurement.value * 1024.0 * 1024.0; }
      else if measurement.unit == "GB" { return measurement.value * 1024.0 * 1024.0 * 1024.0; }
      measurement.value
    }
    
    buffer.column_f64(format!("{}_in", prefix).as_str(), bytes(&io.incoming)).expect(format!("Failed to add incoming {} IO", prefix).as_str());
    buffer.column_f64(format!("{}_out", prefix).as_str(), bytes(&io.outgoing)).expect(format!("Failed to add outgoing {} IO", prefix).as_str());
  }
  
  fn add_memory(buffer: &mut Buffer, measurement: &Measurement, column: &str)
  {
    if measurement.unit == "B" { buffer.column_f64(column, measurement.value).expect("Failed to add memory B"); }
    if measurement.unit == "KiB" { buffer.column_f64(column, measurement.value * 1024.0).expect("Failed to add memory KiB"); }
    if measurement.unit == "MiB" { buffer.column_f64(column, measurement.value * 1024.0 * 1024.0).expect("Failed to add memory MiB"); }
    if measurement.unit == "GiB" { buffer.column_f64(column, measurement.value * 1024.0 * 1024.0 * 1024.0).expect("Failed to add memory GiB"); }
  }
  
  for stat in &stats
  {
    buffer.table(table)?.
        symbol(chost, cli.host.clone())?.
        symbol(ccontainer, stat.container.clone())?.
        symbol(cname, stat.name.clone())?.
        column_str(cid, stat.id.clone())?.
        column_f64(ccpu, stat.cpuPercentage)?.
        column_f64(cmp, stat.memoryPercentage)?.
        column_i64(cpids, stat.pids as i64)?;
    
    add_io(&mut buffer, &stat.blockIO, "block_io");
    add_io(&mut buffer, &stat.netIO, "net_io");
    add_memory(&mut buffer, &stat.memoryUsage, "memory_use");
    add_memory(&mut buffer, &stat.totalMemory, "total_memory");
    
    buffer.at(TimestampNanos::from_datetime(time)?)?;
  }
  
  disk_usage(cli, &mut buffer, time)?;

  sender.flush(&mut buffer)?;
  info!("Published {:?} container statistics for {}.", stats.len(), cli.host);
  Ok(())
}

fn disk_usage(cli: &Cli, buf: &mut Buffer, time: DateTime<Utc>) -> Result<()>
{
  if cli.disks.is_empty() { return Ok(()); }

  let table = TableName::new(cli.disk_table.as_str())?;
  let chost = ColumnName::new("host")?;
  let cname = ColumnName::new("name")?;
  let cfs = ColumnName::new("file_system")?;
  let cmp = ColumnName::new("mount_point")?;
  let ctype = ColumnName::new("type")?;
  let cas = ColumnName::new("available_space")?;
  let cpu = ColumnName::new("percentage_use")?;
  let cr = ColumnName::new("read_bytes")?;
  let cw = ColumnName::new("write_bytes")?;
  
  let disks = Disks::new_with_refreshed_list();
  for disk in disks.list()
  {
    for name in &cli.disks
    {
      if disk.name() ==  OsString::from(name)
      {
        buf.table(table)?.
            symbol(chost, cli.host.clone())?.
            symbol(cname, disk.name().to_str().unwrap().to_string())?.
            symbol(cfs, disk.file_system().to_str().unwrap().to_string())?.
            symbol(cmp, disk.mount_point().to_str().unwrap().to_string())?.
            symbol(ctype, disk.kind().to_string())?.
            column_i64(cas, disk.available_space() as i64)?.
            column_f64(cpu, (disk.available_space() as f64)/(disk.total_space() as f64) * 100.0)?.
            column_i64(cr, disk.usage().read_bytes as i64)?.
            column_i64(cw, disk.usage().written_bytes as i64)?.
            at(TimestampNanos::from_datetime(time)?)?;
        info!("Added disk statistics for {} on {}.", name, cli.host);
      }
    }
  }
  
  Ok(())
}