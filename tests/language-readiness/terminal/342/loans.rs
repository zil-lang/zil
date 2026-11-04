//! Real unused definitions must carry the same alias obligations as instances.
use super::*;

#[test]
fn terminal342_unused_generic_aliases_are_checked_before_instantiation() {
    let negatives = [
        include_str!("../../mutable337/known-incomplete-unused-generic.zil"),
        "fn bad<T:Copy>(p:&mut T)->T{let a=&mut *p;let b=&mut *p;return *a}fn main(){}",
        "fn bad<T:Copy>(p:&mut T)->T{let a=&*p;let b=&mut *p;return *a}fn main(){}",
        "fn bad<T:Copy>(p:&mut T,flag:bool)->T{let a=&*p;if flag{let b=&mut *p;}return *a}fn main(){}",
        "fn bad<T:Copy>(p:T)->T{var x=p;let a=&mut x;let v=x;return *a}fn main(){}",
        "fn bad<T:Copy>(p:T)->T{var x=[p,p];let a=&mut x[0];let v=x[1];return *a}fn main(){}",
        "fn pair<T:Copy>(a:&mut T,b:&mut T){}fn bad<T:Copy>(p:&mut T){pair(p,p)}fn main(){}",
        "fn bad<T:Copy>(p:&mut T,flag:bool)->T{let a=&*p;while flag{let b=&mut *p;}return *a}fn main(){}",
    ];
    for text in negatives {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{text}: {}",
            report.to_json()
        );
        assert_eq!(
            report.phase,
            zil_base::Phase::Expand,
            "{text}: {}",
            report.to_json()
        );
        assert!(report.artifacts.is_empty(), "{}", report.to_json());
    }
}

#[test]
fn terminal342_generic_windows_end_at_last_use_and_keep_disjoint_parameters() {
    for text in [
        "fn read<T:Copy>(p:&mut T)->T{let a=&*p;let value=*a;let b=&mut *p;return *b}fn result()->i32{var x=42;return read(&mut x)}",
        "fn read<T:Copy>(p:T)->T{var x=p;let a=&mut x;let value=*a;x=value;return x}fn result()->i32{return read(42)}",
        "fn choose<T:Copy>(p:&mut T,q:&mut T)->T{let a=&mut *p;let b=&mut *q;*a=*b;return *a}fn result()->i32{var x=1;var y=42;return choose(&mut x,&mut y)}",
        "fn relay<T:Copy>(p:&mut T)->&mut T{let a=&mut *p;return a}fn result()->i32{var x=42;return *relay(&mut x)}",
        "fn read<T:Copy>(p:&mut T,flag:bool)->T{if flag{let a=&mut *p;return *a}let b=&mut *p;return *b}fn result()->i32{var x=42;return read(&mut x,false)}",
        "fn read<T:Copy>(p:&mut T,flag:bool)->T{while flag{let a=&mut *p;let value=*a;}return *p}fn result()->i32{var x=42;return read(&mut x,false)}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{text}: {}",
            report.to_json()
        );
    }
}
