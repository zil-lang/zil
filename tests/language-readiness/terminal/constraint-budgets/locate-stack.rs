unsafe extern "C" {
    fn setup_abort_trace();
}
// Temporary actual-stage diagnostic consumer.
fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("source path");
    let stack = args.next().unwrap_or_else(|| "default".into());
    let stage =
        zil_driver::Stage::from_name(&args.next().unwrap_or_else(|| "verify".into())).unwrap();
    let input = std::fs::read_to_string(&path).unwrap();
    let mut builder = std::thread::Builder::new().name("api-request".into());
    if stack != "default" {
        builder = builder.stack_size(stack.parse().unwrap());
    }
    let status = builder
        .spawn(move || {
            unsafe {
                setup_abort_trace();
            }
            let source =
                zil_base::SourceUnit::new(zil_base::FileId(0), zil_base::ModuleId(0), path, input);
            let options = zil_driver::CompileOptions {
                stop_after: stage,
                ..zil_driver::CompileOptions::default()
            };
            let report = zil_driver::compile_source(&source, &options);
            println!("{}", report.to_json());
            report.status == zil_base::ResultCategory::Success
                && (stage != zil_driver::Stage::Verify || report.verification_runs == 1)
        })
        .unwrap()
        .join()
        .unwrap();
    if !status {
        std::process::exit(1);
    }
}
