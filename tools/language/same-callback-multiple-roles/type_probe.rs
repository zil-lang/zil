//! Source-to-Types diagnostic utility. This deliberately does not report Verify
//! or native acceptance; the driver integration tests provide those gates.
use std::{env, fs, path::PathBuf};
use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
fn main() {
    let mut args = env::args_os().skip(1);
    let path = PathBuf::from(args.next().expect("source path"));
    let output = PathBuf::from(args.next().expect("phase output directory"));
    assert!(args.next().is_none());
    fs::create_dir_all(&output).unwrap();
    let unit = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        path.to_string_lossy(),
        fs::read_to_string(&path).unwrap(),
    );
    let parsed = match zil_syntax::parse(&unit).into_result() {
        Ok(ast) => ast,
        Err(ds) => {
            eprintln!("{ds:#?}");
            std::process::exit(1)
        }
    };
    let core = match zil_expand::expand(&unit, &parsed) {
        Ok(core) => core,
        Err(ds) => {
            eprintln!("{ds:#?}");
            std::process::exit(2)
        }
    };
    fs::write(output.join("core.txt"), format!("{core:#?}")).unwrap();
    let typed = match zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()) {
        Ok(typed) => typed,
        Err(ds) => {
            eprintln!("{ds:#?}");
            std::process::exit(3)
        }
    };
    fs::write(output.join("typed.txt"), format!("{typed:#?}")).unwrap();
    println!(
        "Types: Success; functions={}; closures={}; Verify: NotRun; native=0",
        typed.functions.len(),
        typed.closures.len()
    );
}
