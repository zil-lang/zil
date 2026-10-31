//! Standalone release-linked probe; observer intervals include intervening driver
//! work, not exclusive per-phase CPU time. No performance acceptance thresholds.
use std::{collections::BTreeMap, path::PathBuf, time::Instant};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompilationSession, CompileOptions, ProjectSources, Stage};
// Linux diagnostic only: process CPU includes compiler threads but excludes
// child linkers/native controls. It is not total build-tree CPU time.
#[repr(C)]
struct ProcessTime {
    seconds: std::ffi::c_long,
    nanoseconds: std::ffi::c_long,
}
unsafe extern "C" {
    fn clock_gettime(clock: i32, value: *mut ProcessTime) -> i32;
}
fn process_cpu_ns() -> u128 {
    let mut value = ProcessTime {
        seconds: 0,
        nanoseconds: 0,
    };
    // CLOCK_PROCESS_CPUTIME_ID=2 on the Linux host already required by /proc.
    // ProcessTime has the native timespec layout and writable storage.
    assert_eq!(unsafe { clock_gettime(2, &mut value) }, 0);
    value.seconds as u128 * 1_000_000_000 + value.nanoseconds as u128
}
fn leaf(i: usize, functions: usize, edit: usize) -> String {
    let mut text = format!(
        "fn h0()->i32{{return {}{};}}\n",
        i,
        if edit == 0 {
            String::new()
        } else {
            format!("+{edit}")
        }
    );
    for f in 1..functions - 1 {
        text += &format!("fn h{f}()->i32{{return h{}();}}\n", f - 1);
    }
    text += &format!("pub fn value()->i32{{return h{}();}}\n", functions - 2);
    text
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let modules: usize = args[1].parse().unwrap();
    let functions: usize = args[2].parse().unwrap();
    let capacity: usize = args[3].parse().unwrap();
    let iterations: usize = args[4].parse().unwrap();
    let root = PathBuf::from(&args[5]);
    std::fs::create_dir_all(&root).unwrap();
    let mut options = CompileOptions::default();
    if args.get(6).is_some_and(|s| s == "build") {
        options.stop_after = Stage::Build;
        options.backend = zil_driver::NativeBackend::LlvmApi;
        options.entry = Some("value".into());
        options.output = Some(root.join("program"));
        options.object_cache = Some(root.join("objects"));
        options.codegen.units = 8;
        options.codegen.jobs = 2;
    }
    let mut sources = Vec::new();
    let mut paths = BTreeMap::new();
    let mut main = String::new();
    for i in 1..modules {
        main += &format!("import m{i}\n");
    }
    main += "fn value()->i32{var total:i32=0;\n";
    for i in 1..modules {
        main += &format!("total=total+m{i}.value();\n");
    }
    main += "return total;}\n";
    for i in 0..modules {
        paths.insert(vec![format!("m{i}")], ModuleId(i as u32));
        sources.push(SourceUnit::new(
            FileId(i as u32),
            ModuleId(i as u32),
            root.join(format!("m{i}.zil")).to_string_lossy(),
            if i == 0 {
                main.clone()
            } else {
                leaf(i, functions, 0)
            },
        ));
    }
    let mut project = ProjectSources {
        root: ModuleId(0),
        modules: sources,
        paths,
    };
    let mut session = CompilationSession::with_capacity(capacity);
    for request in 0..iterations + 3 {
        let (label, edit) = match request {
            0 => ("cold", 0),
            1 => ("unchanged", 0),
            n if n == iterations + 2 => ("restore", 0),
            n => ("private", n - 1),
        };
        project.modules[1].text = leaf(1, functions, edit);
        let mut times = BTreeMap::<String, (u128, usize, usize)>::new();
        let cpu_started = process_cpu_ns();
        let started = Instant::now();
        let mut previous = started;
        let report = session.compile_project_with_observer(&project, &options, &mut |event| {
            let now = Instant::now();
            let cell = times.entry(event.phase.as_str().into()).or_default();
            cell.0 += now.duration_since(previous).as_nanos();
            if event.disposition.as_str() == "reused" {
                cell.2 += 1;
            } else {
                cell.1 += 1;
            }
            previous = now;
        });
        let elapsed = started.elapsed().as_nanos();
        let process_cpu = process_cpu_ns() - cpu_started;
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        if options.stop_after == Stage::Build {
            let output = std::process::Command::new(root.join("program"))
                .output()
                .unwrap();
            assert!(output.status.success());
            assert_eq!(
                String::from_utf8(output.stdout).unwrap(),
                format!("{}\n", (1..modules).sum::<usize>() + edit)
            );
        }
        let phases=times.iter().map(|(phase,(ns,executed,reused))|format!("\"{phase}\":{{\"interval_ns\":{ns},\"executed\":{executed},\"reused\":{reused}}}")).collect::<Vec<_>>().join(",");
        let rss = std::fs::read_to_string("/proc/self/status")
            .unwrap()
            .lines()
            .filter(|line| line.starts_with("VmRSS:"))
            .flat_map(|line| line.split_whitespace().nth(1))
            .next()
            .unwrap()
            .to_owned();
        println!(
            "{{\"request\":\"{label}\",\"edit\":{edit},\"modules\":{modules},\"functions_per_leaf\":{functions},\"capacity\":{capacity},\"elapsed_ns\":{elapsed},\"process_cpu_ns\":{process_cpu},\"retained_raw\":{},\"rss_kib\":{rss},\"phases\":{{{phases}}}}}",
            session.cached_query_count()
        );
    }
}
