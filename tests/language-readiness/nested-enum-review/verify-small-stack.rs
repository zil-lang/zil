//! Standalone API consumer of existing prebuilt rlibs; no Cargo target is added.
fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("source path");
    let stack = args.next().unwrap_or_else(|| "2097152".into()).parse().unwrap();
    let input = std::fs::read_to_string(&path).unwrap();
    let status = std::thread::Builder::new().name("api-verify".into()).stack_size(stack)
        .spawn(move || {
            let source = zil_base::SourceUnit::new(zil_base::FileId(0), zil_base::ModuleId(0), path, input);
            let report = zil_driver::compile_source(&source, &zil_driver::CompileOptions::default());
            println!("{}", report.to_json());
            report.status == zil_base::ResultCategory::Success && report.verification_runs == 1
        }).unwrap().join().unwrap();
    if !status { std::process::exit(1); }
}
