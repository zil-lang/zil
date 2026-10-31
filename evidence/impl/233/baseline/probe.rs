use std::{collections::BTreeMap, fs};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, ProjectSources, compile_project, compile_source};

fn project(files: &[(&str, &str)]) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: files.iter().enumerate().map(|(n, (name, text))| {
            SourceUnit::new(FileId(n as u32), ModuleId(n as u32), format!("{name}.zil"), *text)
        }).collect(),
        paths: files.iter().enumerate().skip(1).map(|(n, (name, _))| {
            (vec![name.to_string()], ModuleId(n as u32))
        }).collect::<BTreeMap<_, _>>(),
    }
}

fn record(label: &str, files: &[(&str, &str)]) -> ResultCategory {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evidence/impl/233/baseline").join(label);
    fs::create_dir_all(&dir).unwrap();
    for (name, text) in files { fs::write(dir.join(format!("{name}.zil")), text).unwrap(); }
    let report = compile_project(&project(files), &CompileOptions::default());
    fs::write(dir.join("report.json"), report.to_json()).unwrap();
    println!("{label}: {:?} {:?}", report.status, report.phase);
    report.status
}

#[test]
fn record_preimplementation_acceptance_and_budget() {
    let split = [
        ("main", "import rules;import data;fn read<T>(x:&T)->i32 where T:((rules.Code))=>x.code();fn main()->i32{let x=data.make();return read(&x)-13}"),
        ("rules", "pub trait Code{fn code(self:&Self)->i32;}"),
        ("data", "import rules;pub struct Payload{n:i32}impl rules.Code for Payload{fn code(self:&Self)->i32=>self.n}pub fn make()->Payload=>Payload{n:13}"),
    ];
    assert_eq!(record("split-original-where", &split), ResultCategory::Unsupported);
    for name in ["Send", "Sync", "Share", "Transfer", "Drop"] {
        let model = format!("pub trait {name}{{}}pub struct Payload{{n:i32}}impl {name} for Payload{{}}");
        let status = record(&format!("reserved-unused-{name}"), &[("main", "import model;fn main()->i32=>0"), ("model", &model)]);
        assert_eq!(status, ResultCategory::Success, "expected reproduced old misacceptance {name}");
    }
    for name in ["Fn", "FnMut", "FnOnce"] {
        let model = format!("pub trait {name}{{}}");
        let status = record(&format!("reserved-declaration-{name}"), &[("main", "import model;fn main()->i32=>0"), ("model", &model)]);
        assert_eq!(status, ResultCategory::Success, "expected reproduced old misacceptance {name}");
    }
    let controls = [
        ("ordinary", "fn main()->i32=>0"),
        ("drop", "resource R{n:i32}impl Drop for R{fn drop(self:Self){}}fn main()->i32{let r=R{n:1};return 0}"),
        ("local-trait", "trait Code{fn code(self:&Self)->i32;}struct Payload{n:i32}impl Code for Payload{fn code()->i32=>.n}fn main()->i32{let x=Payload{n:13};return x.code()-13}"),
    ];
    let mut entries = Vec::new();
    for (label, text) in controls {
        let unit = SourceUnit::new(FileId(0), ModuleId(0), "main.zil", text);
        let mut low = 1;
        let mut high = 1_000_000;
        while low < high {
            let middle = low + (high-low)/2;
            let mut options = CompileOptions::default();
            options.limits.max_analysis_steps = middle;
            let report = compile_source(&unit, &options);
            match report.status {
                ResultCategory::Success => high = middle,
                ResultCategory::ResourceLimit => low = middle + 1,
                _ => panic!("{}", report.to_json()),
            }
        }
        let mut options = CompileOptions::default();
        options.limits.max_analysis_steps = low;
        assert_eq!(compile_source(&unit, &options).status, ResultCategory::Success);
        options.limits.max_analysis_steps = low - 1;
        assert_eq!(compile_source(&unit, &options).status, ResultCategory::ResourceLimit);
        entries.push(format!("{{\"label\":\"{label}\",\"minimum_analysis_steps\":{low}}}"));
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evidence/impl/233/baseline").join(label);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("main.zil"), text).unwrap();
        println!("{label} budget={low}");
    }
    let old_project = project(&[("main", "import model;fn main()->i32{let x=model.make();return x.code()-13}"), ("model", "pub trait Code{fn code(self:&Self)->i32;}pub struct Payload{n:i32}impl Code for Payload{fn code()->i32=>.n}pub fn make()->Payload=>Payload{n:13}")]);
    let (mut low, mut high) = (1, 1_000_000);
    while low < high {
        let middle = low+(high-low)/2;
        let mut options = CompileOptions::default();
        options.limits.max_analysis_steps = middle;
        match compile_project(&old_project, &options).status {
            ResultCategory::Success => high = middle,
            ResultCategory::ResourceLimit => low = middle+1,
            status => panic!("unexpected project baseline status {status:?}"),
        }
    }
    println!("module-local-trait budget={low}");
    entries.push(format!("{{\"label\":\"module-local-trait\",\"minimum_analysis_steps\":{low}}}"));
    fs::write(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evidence/impl/233/baseline/budgets.json"), format!("[{}]\n", entries.join(","))).unwrap();
}
