use std::{fs, path::Path};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit};

fn main() {
    let root = Path::new("/workspace/scratch/39be3faa4442/run-logs/review165");
    let cases = [
        ("undeclared-copy", "fn id<T: Copy>(x:T)->T=>x\nfn sample()->i32=>0", Some((ResultCategory::SafeReject, "E-GENERIC-ARGUMENT", "generic bound Copy is not a declared nominal trait"))),
        ("unused-symbolic-method", "trait Read{fn read(self:&Self)->i32;}fn unused<T:Read>(x:&T)->i32=>x.read()\nfn sample()->i32=>0", None),
        ("unknown-unused-attribute", "#[independent_unknown]\nfn id<T>(x:T)->T=>x\nfn sample()->i32=>0", Some((ResultCategory::Unsupported, "E-UNSUPPORTED", "unexecuted generic declaration attribute"))),
        ("declared-copy-cannot-grant-capability", "trait Copy{}fn id<T:Copy>(x:T)->T=>x\nfn sample()->i32=>0", Some((ResultCategory::SafeReject, "E-GENERIC-ARGUMENT", "nominal bounds cannot manufacture built-in capabilities"))),
        ("unused-bound-cannot-invent-member", "trait Read{fn read(self:&Self)->i32;}fn unused<T:Read>(x:&T)->i32=>x.missing()\nfn sample()->i32=>0", Some((ResultCategory::SafeReject, "E-GENERIC-ARGUMENT", "not provided by the declared trait bounds"))),
    ];
    for (name, text, expected) in cases {
        fs::write(root.join(format!("{name}.zil")), text).unwrap();
        let source = SourceUnit::new(FileId(0), ModuleId(0), format!("{name}.zil"), text);
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        let result = zil_expand::expand_with_map(&source, &parsed);
        if let Some((category, code, message)) = expected {
            let errors = result.unwrap_err();
            fs::write(root.join(format!("{name}-diagnostics.txt")), format!("{errors:#?}\n")).unwrap();
            assert!(errors.iter().any(|error| error.phase == Phase::Expand && error.category == category && error.code == code && error.message.contains(message)), "{name}: {errors:?}");
            println!("PASS {name}: {errors:?}");
        } else {
            let expanded = result.unwrap();
            assert!(expanded.core.unsupported.is_empty());
            assert_eq!(expanded.core.functions.len(), 1, "unused generic must be checked without creating a concrete instance");
            assert_eq!(expanded.core.functions[0].name, "sample");
            expanded.source_map.validate(&[source], &Limits::default()).unwrap();
            fs::write(root.join(format!("{name}-core.txt")), format!("{:#?}\n", expanded.core)).unwrap();
            println!("PASS {name}: checked bound and symbolic method with no instance");
        }
    }
}
