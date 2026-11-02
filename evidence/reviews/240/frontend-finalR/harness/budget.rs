use std::{fs, path::Path};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
fn threshold(mut high:usize, mut run: impl FnMut(usize) -> Result<String, Vec<zil_base::Diagnostic>>) -> (usize, String) {
    let mut low = 0;
    let reference = run(high).unwrap();
    while low + 1 < high {
        let middle = low + (high - low) / 2;
        match run(middle) {
            Ok(value) => { assert_eq!(value, reference); high = middle; }
            Err(errors) => { assert!(errors.iter().all(|e| e.category == ResultCategory::ResourceLimit), "{errors:?}"); low = middle; }
        }
    }
    assert_eq!(run(high).unwrap(), reference);
    let errors = run(high - 1).unwrap_err();
    assert!(errors.iter().all(|e| e.category == ResultCategory::ResourceLimit));
    assert_eq!(run(high).unwrap(), reference);
    assert_eq!(run(high).unwrap(), reference);
    (high, reference)
}
fn main() {
    let base=Path::new("/tmp/review240-boundaries");
    let out=base.join(std::env::var("REVIEW240_BUDGET_OUTPUT").unwrap());fs::create_dir_all(&out).unwrap();
    for name in ["old-copy-enum", "old-give-param", "old-package-field", "desc-live"] {
        let text=fs::read_to_string(base.join("budget-sources").join(format!("{name}.zil"))).unwrap();
        let source=SourceUnit::new(FileId(0),ModuleId(0),"helper-budget.zil",text);
        let syntax=zil_syntax::parse(&source).into_result().unwrap();
        let (work,core_text)=threshold(Limits::default().max_analysis_steps,|n|zil_expand::expand_with_map_limits(&source,&syntax,&Limits{max_analysis_steps:n,..Limits::default()}).map(|v|format!("{:?}",v.core)));
        let (nodes,_)=threshold(Limits::default().max_nodes,|n|zil_expand::expand_with_map_limits(&source,&syntax,&Limits{max_nodes:n,..Limits::default()}).map(|v|format!("{:?}",v.core)));
        let core=zil_expand::expand(&source,&zil_syntax::parse(&source).into_result().unwrap()).unwrap();
        assert_eq!(format!("{core:?}"),core_text);
        let (types,typed_text)=threshold(Limits::default().max_analysis_steps,|n|zil_types::check(&core,&TargetProfile::host_llvm(),&Limits{max_analysis_steps:n,..Limits::default()}).map(|v|format!("{v:?}")));
        fs::write(out.join(format!("{name}.core")),core_text).unwrap();
        fs::write(out.join(format!("{name}.typed")),typed_text).unwrap();
        let row=format!("{{\"profile\":\"{name}\",\"expand_work\":{work},\"expand_nodes\":{nodes},\"types_work\":{types},\"N_minus_one\":\"ResourceLimit\",\"warm_and_fresh_N\":\"identical output\"}}");
        println!("{row}");fs::write(out.join(format!("{name}.json")),row).unwrap();
    }
}
