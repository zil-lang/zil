//! Independent review of concrete-address member reads and generic ownership.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, compile_source};
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn check(text: &str, accepted: bool) {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = root.join(format!(
        "review181-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("input.zil"), text).unwrap();
    let source = SourceUnit::new(FileId(0), ModuleId(0), "projection181.zil", text);
    let report = compile_source(
        &source,
        &CompileOptions {
            emit_dir: Some(path.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(path.join("compile.json"), report.to_json()).unwrap();
    if accepted {
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1)
    } else {
        assert!(
            matches!(
                report.status,
                ResultCategory::SafeReject | ResultCategory::Unsupported
            ),
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0)
    }
}
#[test]
fn reviewer_generic_value_projection_must_not_duplicate_unconstrained_t() {
    for definition in [
        "fn bad<T>(holder:Holder<T>)->(T,T){return (holder.value,holder.value)}",
        "fn bad<T>(holder:Holder<T>)->(T,T){return ((holder).value,(holder).value)}",
        "fn bad<T>(holder:Holder<T>)->T{let first=holder.value;return holder.value}",
        "fn bad<T>(holder:Holder<T>)->Holder<T>{let first=holder.value;return holder}",
    ] {
        check(
            &format!("struct Holder<T>{{value:T}}{definition}fn main(){{}}"),
            false,
        );
    }
}
#[test]
fn reviewer_copy_address_projection_does_not_move_exclusive_address() {
    check(
        "resource R{n:i32}package Item{manages r:R;value:i32}fn step<U>(p:&mut Item)->i32{let before=p.value;p.value=before+1;let after=p.value;return before+after}fn main()->i32{let s:Store<Item>=store_new();store_reserve(s,1);store_push(s,Item{r:R{n:0},value:7});let p=store_at_mut(s,0);let n=step::<i32>(p);return n+p.value-23}",
        true,
    );
}
#[test]
fn reviewer_borrowed_projection_cannot_grant_managed_or_unknown_copy() {
    for declaration in [
        "fn bad<T>(holder:&Holder<T>)->T=>holder.value;",
        "fn bad<T>(holder:&mut Holder<T>)->T=>holder.value;",
        "fn bad<T>(p:&Item)->R=>p.r;",
        "fn bad<T>(p:&mut Item)->R=>p.r;",
        "fn bad<T:StoreElement>(p:&mut T)->i32=>p.value;",
        "fn bad<T>(p:&mut Item)->i32{let moved=p;return p.value}",
        "fn bad<T>(p:&mut Item){let first=p;let second=p;}",
    ] {
        check(
            &format!(
                "struct Holder<T>{{value:T}}resource R{{n:i32}}package Item{{manages r:R;value:i32}}{declaration}fn main(){{}}"
            ),
            false,
        );
    }
}

#[test]
fn reviewer_effectful_address_base_is_evaluated_once_per_projection() {
    use std::process::Command;
    use zil_driver::{Optimization, Stage};
    let text = "struct Point{x:i32}fn touch(p:&mut Point,n:&mut i32)->&mut Point{*n+=1;return p}fn read<T>(p:&mut Point,n:&mut i32)->i32{return touch(p,n).x+touch(p,n).x}fn main()->i32{var point=Point{x:7};var count=0;let value=read::<i32>(&mut point,&mut count);return value+count-16}";
    for optimization in [Optimization::O0, Optimization::O2] {
        let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = root.join(format!(
            "review181-native-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("input.zil"), text).unwrap();
        let executable = path.join("program");
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "effect181.zil", text),
            &CompileOptions {
                stop_after: Stage::Build,
                optimization,
                output: Some(executable.clone()),
                emit_dir: Some(path.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(path.join("compile.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let observed = Command::new(executable).output().unwrap();
        fs::write(path.join("stdout"), &observed.stdout).unwrap();
        fs::write(path.join("stderr"), &observed.stderr).unwrap();
        fs::write(
            path.join("exit.txt"),
            format!("{:?}\n", observed.status.code()),
        )
        .unwrap();
        assert_eq!(observed.status.code(), Some(0));
        assert!(observed.stdout.is_empty());
        assert!(observed.stderr.is_empty());
    }
}
