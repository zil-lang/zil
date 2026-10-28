//! Fixed four-request whole compiler session. Output and edits are outside API clocks.
use std::{collections::BTreeMap, fs, path::PathBuf, time::Instant};
use zil_base::ResultCategory;
use zil_driver::{CompilationSession, CompileOptions, Stage};
struct Restore(BTreeMap<PathBuf, Vec<u8>>);
impl Drop for Restore { fn drop(&mut self) { for (path, bytes) in &self.0 { fs::write(path, bytes).expect("restore original input"); } } }
fn main() {
 let a:Vec<_>=std::env::args().collect(); assert_eq!(a.len(),6,"out verify|verify-emit root edits capacity");
 let out=PathBuf::from(&a[1]); fs::create_dir_all(&out).unwrap(); let root=PathBuf::from(&a[3]); let edits=PathBuf::from(&a[4]);
 let mut original=Restore(BTreeMap::new()); let mut session=CompilationSession::with_capacity(a[5].parse().unwrap()); let mut records=Vec::new(); let mut complete=true;
 for scenario in ["cold","unchanged","private_edit","public_edit"] {
  if scenario.ends_with("_edit") { for line in fs::read_to_string(edits.join(format!("{scenario}.tsv"))).unwrap().lines() { let (path,replacement)=line.split_once('\t').unwrap(); let path=PathBuf::from(path); original.0.entry(path.clone()).or_insert_with(||fs::read(&path).unwrap()); fs::write(path,fs::read(replacement).unwrap()).unwrap(); } }
  let options=CompileOptions{stop_after:Stage::Verify,emit_dir:(a[2]=="verify-emit").then(||out.join(scenario)),..CompileOptions::default()};
  let start=Instant::now(); let report=session.compile_path(&root,&options); let elapsed=start.elapsed().as_nanos(); let active=session.active_query_count();
  let good=report.status==ResultCategory::Success && report.verification_runs==1 && active==0; complete &= good;
  records.push(format!("{{\"scenario\":\"{scenario}\",\"api_wall_ns\":{elapsed},\"active_queries\":{active},\"cached_queries\":{},\"report\":{}}}",session.cached_query_count(),report.to_json()));
  if !good {break}
 }
 drop(session); drop(original);
 let rss=fs::read_to_string("/proc/self/status").unwrap_or_default().lines().find(|l|l.starts_with("VmHWM:")).and_then(|l|l.split_whitespace().nth(1)).and_then(|s|s.parse::<u64>().ok()).unwrap_or(0);
 fs::write(out.join("report.json"),format!("{{\"complete\":{complete},\"vmhwm_kib\":{rss},\"records\":[{}]}}",records.join(","))).unwrap();
 if !complete {std::process::exit(1)}
}
