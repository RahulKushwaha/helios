//! A YCSB (Yahoo! Cloud Serving Benchmark) driver for the B+Tree.
//!
//! Implements the standard core workloads over the `Db` API and reports
//! throughput and latency percentiles for a load phase and a run phase.
//!
//! ```text
//!   A  read/update      50/50    zipfian
//!   B  read/update      95/5     zipfian
//!   C  read-only        100      zipfian
//!   D  read/insert      95/5     latest (reads skew to recent inserts)
//!   E  scan/insert      95/5     zipfian (short range scans)
//!   F  read/read-modify-write  50/50  zipfian
//! ```
//!
//! Run (harness is disabled, so args pass straight through):
//!
//! ```text
//!   cargo bench -p btree --bench ycsb                 # all workloads, defaults
//!   cargo bench -p btree --bench ycsb -- A            # one workload
//!   cargo bench -p btree --bench ycsb -- all 200000 200000 100
//!                                          ^wl  ^recs  ^ops    ^value bytes
//! ```
//!
//! Keys are FNV-hashed integer ids (`user<20 digits>`), so hot zipfian ids are
//! scattered across the keyspace as in real YCSB. Durability model: the load
//! phase is `sync`ed before the run, so reads hit disk; run-phase writes buffer
//! in the pager and are flushed at the end (write-back).

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use btree::Db;

// --- RNG + distributions ---------------------------------------------------

/// xorshift64 (keeps the crate dependency-free).
struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed | 1)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
    /// Uniform in [0, 1).
    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// YCSB Zipfian generator (Gray et al.), returns ids in `[0, n)` with a heavy
/// head at id 0. Callers hash the result, so the popular ids are scattered.
struct Zipfian {
    n: f64,
    theta: f64,
    zetan: f64,
    alpha: f64,
    eta: f64,
}
impl Zipfian {
    fn new(n: u64) -> Zipfian {
        let theta = 0.99;
        let zeta2 = zeta(2, theta);
        let zetan = zeta(n, theta);
        let nf = n as f64;
        let alpha = 1.0 / (1.0 - theta);
        let eta = (1.0 - (2.0 / nf).powf(1.0 - theta)) / (1.0 - zeta2 / zetan);
        Zipfian {
            n: nf,
            theta,
            zetan,
            alpha,
            eta,
        }
    }
    fn next(&self, rng: &mut Rng) -> u64 {
        let u = rng.unit();
        let uz = u * self.zetan;
        if uz < 1.0 {
            return 0;
        }
        if uz < 1.0 + 0.5f64.powf(self.theta) {
            return 1;
        }
        (self.n * (self.eta * u - self.eta + 1.0)).powf(self.alpha) as u64
    }
}

fn zeta(n: u64, theta: f64) -> f64 {
    let mut sum = 0.0;
    for i in 1..=n {
        sum += 1.0 / (i as f64).powf(theta);
    }
    sum
}

#[derive(Clone, Copy, PartialEq)]
enum Dist {
    Zipfian,
    Latest,
}

/// FNV-1a over the 8 bytes of `id`, used to scatter sequential ids into keys.
fn fnv1a(mut x: u64) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for _ in 0..8 {
        h = (h ^ (x & 0xff)).wrapping_mul(0x0000_0100_0000_01b3);
        x >>= 8;
    }
    h
}

fn key_for(id: u64) -> Vec<u8> {
    format!("user{:020}", fnv1a(id)).into_bytes()
}

// --- workload definitions --------------------------------------------------

#[derive(Clone, Copy)]
enum Op {
    Read,
    Update,
    Insert,
    Scan,
    Rmw,
}

struct Workload {
    name: &'static str,
    /// Human description for the report.
    mix: &'static str,
    /// Cumulative thresholds in [0,1] for read, update, insert, scan, rmw.
    read: f64,
    update: f64,
    insert: f64,
    scan: f64,
    dist: Dist,
}
impl Workload {
    fn pick(&self, r: f64) -> Op {
        let mut t = self.read;
        if r < t {
            return Op::Read;
        }
        t += self.update;
        if r < t {
            return Op::Update;
        }
        t += self.insert;
        if r < t {
            return Op::Insert;
        }
        t += self.scan;
        if r < t {
            return Op::Scan;
        }
        Op::Rmw
    }
}

fn workloads() -> Vec<Workload> {
    vec![
        Workload {
            name: "A",
            mix: "50% read / 50% update, zipfian",
            read: 0.50,
            update: 0.50,
            insert: 0.0,
            scan: 0.0,
            dist: Dist::Zipfian,
        },
        Workload {
            name: "B",
            mix: "95% read / 5% update, zipfian",
            read: 0.95,
            update: 0.05,
            insert: 0.0,
            scan: 0.0,
            dist: Dist::Zipfian,
        },
        Workload {
            name: "C",
            mix: "100% read, zipfian",
            read: 1.00,
            update: 0.0,
            insert: 0.0,
            scan: 0.0,
            dist: Dist::Zipfian,
        },
        Workload {
            name: "D",
            mix: "95% read / 5% insert, latest",
            read: 0.95,
            update: 0.0,
            insert: 0.05,
            scan: 0.0,
            dist: Dist::Latest,
        },
        Workload {
            name: "E",
            mix: "95% scan / 5% insert, zipfian",
            read: 0.0,
            update: 0.0,
            insert: 0.05,
            scan: 0.95,
            dist: Dist::Zipfian,
        },
        Workload {
            name: "F",
            mix: "50% read / 50% read-modify-write, zipfian",
            read: 0.50,
            update: 0.0,
            insert: 0.0,
            scan: 0.0,
            dist: Dist::Zipfian,
        },
    ]
}

// --- stats -----------------------------------------------------------------

struct Stats {
    ops: u64,
    elapsed: Duration,
    lat_ns: Vec<u64>, // sorted
}
impl Stats {
    fn throughput(&self) -> f64 {
        self.ops as f64 / self.elapsed.as_secs_f64()
    }
    fn pct(&self, p: f64) -> u64 {
        if self.lat_ns.is_empty() {
            return 0;
        }
        let idx = ((p / 100.0) * self.lat_ns.len() as f64) as usize;
        self.lat_ns[idx.min(self.lat_ns.len() - 1)]
    }
}

// --- phases ----------------------------------------------------------------

fn load(db: &mut Db, records: u64, value: &[u8]) -> Stats {
    let mut lat = Vec::with_capacity(records as usize);
    let start = Instant::now();
    for id in 0..records {
        let t = Instant::now();
        db.insert(&key_for(id), value).unwrap();
        lat.push(t.elapsed().as_nanos() as u64);
    }
    db.sync().unwrap();
    let elapsed = start.elapsed();
    lat.sort_unstable();
    Stats {
        ops: records,
        elapsed,
        lat_ns: lat,
    }
}

fn run(db: &mut Db, w: &Workload, records: u64, ops: u64, value: &[u8]) -> Stats {
    let zipf = Zipfian::new(records);
    let mut rng = Rng::new(0x1234_5678_9abc_def0 ^ w.name.as_bytes()[0] as u64);
    let mut lat = Vec::with_capacity(ops as usize);
    let mut next_insert = records;

    // Pick a target id among existing records per the workload distribution.
    let target = |zipf: &Zipfian, rng: &mut Rng, max: u64| -> u64 {
        match w.dist {
            Dist::Zipfian => zipf.next(rng),
            Dist::Latest => (max - 1).saturating_sub(zipf.next(rng)),
        }
    };

    let start = Instant::now();
    for _ in 0..ops {
        let r = rng.unit();
        let op = w.pick(r);
        let t = Instant::now();
        match op {
            Op::Read => {
                let id = target(&zipf, &mut rng, next_insert);
                let _ = db.get(&key_for(id)).unwrap();
            }
            Op::Update => {
                let id = target(&zipf, &mut rng, next_insert);
                db.insert(&key_for(id), value).unwrap();
            }
            Op::Insert => {
                let id = next_insert;
                next_insert += 1;
                db.insert(&key_for(id), value).unwrap();
            }
            Op::Scan => {
                let id = target(&zipf, &mut rng, next_insert);
                let len = (rng.below(100) + 1) as usize;
                let _ = db.scan_limit(Some(&key_for(id)), len).unwrap();
            }
            Op::Rmw => {
                let id = target(&zipf, &mut rng, next_insert);
                let k = key_for(id);
                let _ = db.get(&k).unwrap();
                db.insert(&k, value).unwrap();
            }
        }
        lat.push(t.elapsed().as_nanos() as u64);
    }
    db.sync().unwrap();
    let elapsed = start.elapsed();
    lat.sort_unstable();
    Stats {
        ops,
        elapsed,
        lat_ns: lat,
    }
}

// --- per-workload summary --------------------------------------------------

struct Summary {
    name: &'static str,
    mix: &'static str,
    load_tput: f64,
    run_tput: f64,
    avg_us: f64,
    p50_us: f64,
    p95_us: f64,
    p99_us: f64,
    p999_us: f64,
    max_us: f64,
}
impl Summary {
    fn from(w: &Workload, load: &Stats, run: &Stats) -> Summary {
        let us = |ns: u64| ns as f64 / 1000.0;
        let avg = run.lat_ns.iter().sum::<u64>() as f64 / run.lat_ns.len().max(1) as f64 / 1000.0;
        Summary {
            name: w.name,
            mix: w.mix,
            load_tput: load.throughput(),
            run_tput: run.throughput(),
            avg_us: avg,
            p50_us: us(run.pct(50.0)),
            p95_us: us(run.pct(95.0)),
            p99_us: us(run.pct(99.0)),
            p999_us: us(run.pct(99.9)),
            max_us: us(*run.lat_ns.last().unwrap_or(&0)),
        }
    }
}

// --- main ------------------------------------------------------------------

fn main() {
    // `cargo bench` passes `--bench`; ignore flags, take positional args.
    let args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| !a.starts_with('-'))
        .collect();
    let which = args.first().cloned().unwrap_or_else(|| "all".to_string());
    let records: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(50_000);
    let ops: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(50_000);
    let value_size: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(100);
    let value = vec![b'x'; value_size];

    let selected: Vec<Workload> = workloads()
        .into_iter()
        .filter(|w| which == "all" || which.eq_ignore_ascii_case(w.name))
        .collect();
    if selected.is_empty() {
        eprintln!("unknown workload {which:?}; use A-F or all");
        std::process::exit(1);
    }

    println!("YCSB on btree: {records} records, {ops} ops/run, {value_size}B values\n");
    println!(
        "{:<5} {:>12} {:>12} {:>10} {:>10} {:>10} {:>10}",
        "wkld", "load ops/s", "run ops/s", "avg us", "p50 us", "p99 us", "p999 us"
    );
    println!("{}", "-".repeat(74));

    let mut summaries = Vec::new();
    for w in &selected {
        let dir =
            std::env::temp_dir().join(format!("btree_ycsb_{}_{}", std::process::id(), w.name));
        let _ = std::fs::remove_file(&dir);

        let mut db = Db::open(&dir).unwrap();
        let load_stats = load(&mut db, records, &value);
        let run_stats = run(&mut db, w, records, ops, &value);
        drop(db);
        let _ = std::fs::remove_file(&dir);

        let s = Summary::from(w, &load_stats, &run_stats);
        println!(
            "{:<5} {:>12.0} {:>12.0} {:>10.2} {:>10.2} {:>10.2} {:>10.2}",
            s.name, s.load_tput, s.run_tput, s.avg_us, s.p50_us, s.p99_us, s.p999_us,
        );
        summaries.push(s);
    }

    let hw = Hardware::gather();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("benches/reports")
        .join(format!("ycsb-{stamp}"));
    match write_report(&dir, records, ops, value_size, &hw, &summaries) {
        Ok(()) => println!("\nReport written to {}", dir.join("index.html").display()),
        Err(e) => eprintln!("\nfailed to write report: {e}"),
    }
}

// --- hardware config -------------------------------------------------------

struct Hardware {
    cpu_model: String,
    logical_cpus: usize,
    cpu_mhz: Option<f64>,
    mem_total_kb: Option<u64>,
    kernel: String,
    os: String,
}
impl Hardware {
    fn gather() -> Hardware {
        let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
        let field = |key: &str| {
            cpuinfo
                .lines()
                .find(|l| l.starts_with(key))
                .and_then(|l| l.split_once(':'))
                .map(|(_, v)| v.trim().to_string())
        };
        Hardware {
            cpu_model: field("model name").unwrap_or_else(|| "unknown".into()),
            logical_cpus: cpuinfo
                .lines()
                .filter(|l| l.starts_with("processor"))
                .count(),
            cpu_mhz: field("cpu MHz").and_then(|v| v.parse().ok()),
            mem_total_kb: read_first("/proc/meminfo", "MemTotal:")
                .and_then(|v| v.split_whitespace().next().and_then(|n| n.parse().ok())),
            kernel: read_trim("/proc/sys/kernel/osrelease"),
            os: read_first("/etc/os-release", "PRETTY_NAME=")
                .map(|v| v.trim_matches('"').to_string())
                .unwrap_or_else(|| "unknown".into()),
        }
    }

    /// Raw key/value lines for the `hardware.txt` file.
    fn as_text(&self) -> String {
        let mem = self
            .mem_total_kb
            .map(|kb| format!("{:.1} GiB", kb as f64 / 1024.0 / 1024.0))
            .unwrap_or_else(|| "unknown".into());
        let mhz = self
            .cpu_mhz
            .map(|m| format!("{m:.0} MHz"))
            .unwrap_or_else(|| "unknown".into());
        format!(
            "cpu_model:    {}\ncpu_mhz:      {}\nlogical_cpus: {}\nmemory:       {}\nkernel:       {}\nos:           {}\n",
            self.cpu_model, mhz, self.logical_cpus, mem, self.kernel, self.os,
        )
    }
}

fn read_trim(path: &str) -> String {
    std::fs::read_to_string(path)
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown".into())
}

fn read_first(path: &str, key: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()?
        .lines()
        .find(|l| l.starts_with(key))
        .and_then(|l| l.split_once(['=', ':']))
        .map(|(_, v)| v.trim().to_string())
}

// --- report rendering ------------------------------------------------------

fn write_report(
    dir: &std::path::Path,
    records: u64,
    ops: u64,
    value_size: usize,
    hw: &Hardware,
    rows: &[Summary],
) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("hardware.txt"), hw.as_text())?;
    std::fs::write(
        dir.join("index.html"),
        render_html(records, ops, value_size, hw, rows),
    )?;
    Ok(())
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn render_html(
    records: u64,
    ops: u64,
    value_size: usize,
    hw: &Hardware,
    rows: &[Summary],
) -> String {
    let max_tput = rows.iter().map(|r| r.run_tput).fold(1.0_f64, f64::max);
    let mem = hw
        .mem_total_kb
        .map(|kb| format!("{:.1} GiB", kb as f64 / 1024.0 / 1024.0))
        .unwrap_or_else(|| "unknown".into());
    let mhz = hw
        .cpu_mhz
        .map(|m| format!("{m:.0} MHz"))
        .unwrap_or_else(|| "n/a".into());

    let mut table = String::new();
    for r in rows {
        let bar = (r.run_tput / max_tput * 100.0).round();
        let _ = write!(
            table,
            "<tr><td class=wl>{name}</td><td class=mix>{mix}</td>\
             <td class=num>{load:.0}</td>\
             <td class=num>{run:.0}<div class=bar><span style=\"width:{bar}%\"></span></div></td>\
             <td class=num>{avg:.2}</td><td class=num>{p50:.2}</td><td class=num>{p95:.2}</td>\
             <td class=num>{p99:.2}</td><td class=num>{p999:.2}</td><td class=num>{max:.2}</td></tr>",
            name = esc(r.name),
            mix = esc(r.mix),
            load = r.load_tput,
            run = r.run_tput,
            bar = bar,
            avg = r.avg_us,
            p50 = r.p50_us,
            p95 = r.p95_us,
            p99 = r.p99_us,
            p999 = r.p999_us,
            max = r.max_us,
        );
    }

    format!(
        r#"<!doctype html>
<html lang=en>
<head>
<meta charset=utf-8>
<meta name=viewport content="width=device-width, initial-scale=1">
<title>btree YCSB report</title>
<style>
  :root {{ color-scheme: light dark; }}
  body {{ font: 15px/1.5 -apple-system, Segoe UI, Roboto, sans-serif; margin: 2rem auto; max-width: 1000px; padding: 0 1rem; }}
  h1 {{ margin-bottom: .2rem; }}
  .sub {{ color: #888; margin-top: 0; }}
  .table-wrap {{ overflow-x: auto; margin: 1rem 0; }}
  table {{ border-collapse: collapse; width: 100%; min-width: 720px; }}
  th, td {{ padding: .45rem .6rem; border-bottom: 1px solid #8884; text-align: left; }}
  th {{ font-size: .8rem; text-transform: uppercase; letter-spacing: .04em; color: #888; }}
  td.num, th.num {{ text-align: right; font-variant-numeric: tabular-nums; }}
  td.wl {{ font-weight: 700; }}
  td.mix {{ color: #888; font-size: .85rem; }}
  .bar {{ height: 4px; background: #8882; border-radius: 2px; margin-top: 3px; }}
  .bar span {{ display: block; height: 100%; background: #4c8bf5; border-radius: 2px; }}
  dl {{ display: grid; grid-template-columns: max-content 1fr; gap: .2rem 1rem; }}
  dt {{ color: #888; }}
  dd {{ margin: 0; font-variant-numeric: tabular-nums; }}
  .card {{ border: 1px solid #8884; border-radius: 8px; padding: 1rem 1.2rem; margin: 1rem 0; }}
  code {{ background: #8882; padding: .1rem .3rem; border-radius: 3px; }}
</style>
</head>
<body>
<h1>btree &mdash; YCSB report</h1>
<p class=sub>{records} records &middot; {ops} ops/run &middot; {value_size} B values</p>

<div class=card>
<h2>Hardware</h2>
<dl>
  <dt>CPU</dt><dd>{cpu} ({cpus} logical, {mhz})</dd>
  <dt>Memory</dt><dd>{mem}</dd>
  <dt>Kernel</dt><dd>{kernel}</dd>
  <dt>OS</dt><dd>{os}</dd>
</dl>
</div>

<div class=table-wrap>
<table>
<caption>Throughput in operations per second; latency percentiles in microseconds (&micro;s).</caption>
<thead><tr>
  <th>wkld</th><th>mix</th>
  <th class=num>load ops/s</th><th class=num>run ops/s</th>
  <th class=num>avg &micro;s</th><th class=num>p50 &micro;s</th><th class=num>p95 &micro;s</th>
  <th class=num>p99 &micro;s</th><th class=num>p99.9 &micro;s</th><th class=num>max &micro;s</th>
</tr></thead>
<tbody>
{table}
</tbody>
</table>
</div>

<p class=sub>Keys are FNV-hashed integer ids; requests follow each workload's
distribution. The load phase is <code>sync</code>ed to disk before the run, so
reads hit the file; run-phase writes buffer in the pager (write-back). No WAL.</p>
</body>
</html>
"#,
        records = records,
        ops = ops,
        value_size = value_size,
        cpu = esc(&hw.cpu_model),
        cpus = hw.logical_cpus,
        mhz = mhz,
        mem = mem,
        kernel = esc(&hw.kernel),
        os = esc(&hw.os),
        table = table,
    )
}
