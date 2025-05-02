use substring::Substring;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
pub struct RawStats
{
  pub BlockIO: String,
  pub CPUPerc: String,
  pub Container: String,
  pub ID: String,
  pub MemPerc: String,
  pub MemUsage: String,
  pub Name: String,
  pub NetIO: String,
  pub PIDs: String
}

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
pub struct Measurement
{
  pub value: f64,
  pub unit: String
}

impl Measurement
{
  pub fn new() -> Measurement
  {
    Measurement{value: 0.0, unit: String::new()}
  }
}

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
pub struct IO
{
  pub incoming: Measurement,
  pub outgoing: Measurement
}

impl IO
{
  pub fn new() -> IO
  {
    IO{incoming: Measurement::new(), outgoing: Measurement::new()}
  }
}

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
pub struct Stats
{
  pub id: String,
  pub container: String,
  pub name: String,
  pub blockIO: IO,
  pub cpuPercentage: f64,
  pub memoryPercentage: f64,
  pub memoryUsage: Measurement,
  pub totalMemory: Measurement,
  pub netIO: IO,
  pub pids: u32,
}

impl Stats
{
  pub fn new() -> Stats
  {
    Stats{id: String::new(), container: String::new(), name: String::new(), blockIO: IO::new(),
      cpuPercentage: 0.0, memoryPercentage: 0.0, memoryUsage: Measurement::new(),
      totalMemory: Measurement::new(), netIO: IO::new(), pids: 0}
  }
}

#[allow(non_snake_case)]
pub fn createStats(rs: &RawStats) -> Stats
{
  fn parseIO(value: &String, ioType: &str) -> IO
  {
    let mut io = IO::new();
    let parts : Vec<&str> = value.split(" / ").collect();
    if parts.len() != 2 { return io; }

    let parseMeasurement = |part: &str| -> Measurement
    {
      if part.ends_with("GB") || part.ends_with("gB")
      {
        let v = part.substring(0, part.len() - 2);
        return Measurement { value: v.parse::<f64>().expect(format!("Failed to parse {}/{}", part, ioType).as_str()), unit: "GB".to_string() };
      }
      if part.ends_with("MB") || part.ends_with("mB")
      {
        let v = part.substring(0, part.len() - 2);
        return Measurement{value: v.parse::<f64>().expect(format!("Failed to parse {}/{}", part, ioType).as_str()), unit: "MB".to_string()};
      }
      if part.ends_with("KB") || part.ends_with("kB")
      {
        let v = part.substring(0, part.len() - 2);
        return Measurement{value: v.parse::<f64>().expect(format!("Failed to parse {}/{}", part, ioType).as_str()), unit: "KB".to_string()};
      }
      if part.ends_with("B")
      {
        let v = part.substring(0, part.len() - 1);
        return Measurement{value: v.parse::<f64>().expect(format!("Failed to parse {}/{}", part, ioType).as_str()), unit: "B".to_string()};
      }
      
      return Measurement::new();
    };
    
    io.incoming = parseMeasurement(parts[0]);
    io.outgoing = parseMeasurement(parts[1]);
    io 
  }
  
  fn parseMemory(value: &str) -> Measurement
  {
    if value == "0B" || value.len() < 3 { return Measurement{value: 0.0, unit: "bytes".to_string()}; }
    let v = value.substring(0, value.len() - 3);
    Measurement{ value: v.parse::<f64>().expect(format!("Failed to parse MemUsage {}/{}", v, value).as_str()), unit: value.substring(v.len(), value.len()).to_string() }
  }
  
  let mut stats = Stats{ id: rs.ID.clone(), container: rs.Container.clone(), name: rs.Name.clone(), 
    blockIO: IO::new(), cpuPercentage: 0.0, memoryPercentage: 0.0, 
    memoryUsage: Measurement::new(), totalMemory: Measurement::new(),
    netIO: IO::new(), pids: 0};

  stats.cpuPercentage = rs.CPUPerc.replace("%", "").parse::<f64>().expect(format!("Failed to parse CPUPerc {}", rs.CPUPerc).as_str());
  stats.memoryPercentage = rs.MemPerc.replace("%", "").parse::<f64>().expect(format!("Failed to parse CPUPerc {}", rs.MemPerc).as_str());
  stats.pids = rs.PIDs.parse::<u32>().expect(format!("Failed to parse PIDs {}", rs.PIDs).as_str());
  stats.blockIO = parseIO(&rs.BlockIO, "BlockIO");
  stats.netIO = parseIO(&rs.NetIO, "NetIO");
  
  fn parseMemUsage(value: &String) -> Vec<&str>
  {
    let parts : Vec<&str> = value.split(" / ").collect();
    if parts.len() > 1 { return parts; }
    
    let parts : Vec<&str> = value.split("/").collect();
    if parts.len() > 1 { return parts; }
    
    vec![]
  }
  
  let parts = parseMemUsage(&rs.MemUsage);
  if parts.len() > 1
  {
    let mem = parseMemory(parts[0]);
    stats.memoryUsage.value = mem.value;
    stats.memoryUsage.unit = mem.unit.clone();
    
    let mem = parseMemory(parts[1]);
    stats.totalMemory.value = mem.value;
    stats.totalMemory.unit = mem.unit.clone();
  }
  
  stats
}