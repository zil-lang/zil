//! Nonauthor 092 probes. The separate LLVM caller is an internal-ABI oracle,
//! not a newly supported C by-value aggregate or FFI slice interface.
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use zil_backend_llvm::{EmitOptions, EntryPoint};
use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};

static NEXT: AtomicU64 = AtomicU64::new(0);
fn directory(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_REVIEW092_OUT")
        .or_else(|| std::env::var_os("ZIL_TEST_REPORT_DIR"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    fs::create_dir_all(&root).unwrap();
    let dir = root.join(format!(
        "{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&dir).unwrap();
    println!("092 independent artifacts {}", dir.display());
    dir
}
fn emit(text: &str, dir: &Path, entry: Option<&str>) -> String {
    fs::write(dir.join("source.zil"), text).unwrap();
    let source = SourceUnit::new(FileId(0), ModuleId(0), "independent092.zil", text);
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let entry = entry
        .map(|name| {
            EntryPoint::ReturnValue(typed.functions.iter().find(|f| f.name == name).unwrap().id)
        })
        .unwrap_or(EntryPoint::None);
    fs::write(dir.join("typed.txt"), format!("{typed:#?}\n{plan:#?}\n")).unwrap();
    let verified = zil_verify::verify_module(typed, plan, &target, &limits).unwrap();
    let lir = zil_lower::lower(&verified).unwrap();
    fs::write(dir.join("lir.txt"), format!("{lir:#?}\n")).unwrap();
    let llvm =
        zil_backend_llvm::emit_with_options(&verified, &lir, &EmitOptions { entry }).unwrap();
    fs::write(dir.join("callee.ll"), &llvm).unwrap();
    llvm
}
fn command(
    dir: &Path,
    label: &str,
    cmd: &mut Command,
) -> (std::process::ExitStatus, Vec<u8>, Vec<u8>) {
    fs::write(dir.join(format!("{label}.command")), format!("{cmd:?}\n")).unwrap();
    let stdout = dir.join(format!("{label}.stdout"));
    let stderr = dir.join(format!("{label}.stderr"));
    cmd.stdout(Stdio::from(fs::File::create(&stdout).unwrap()))
        .stderr(Stdio::from(fs::File::create(&stderr).unwrap()));
    let mut child = cmd.spawn().unwrap();
    let begin = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if begin.elapsed() > Duration::from_secs(45) {
            let _ = child.kill();
            let _ = child.wait();
            fs::write(dir.join(format!("{label}.status")), "timeout\n").unwrap();
            panic!("command timeout: {label}");
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    fs::write(dir.join(format!("{label}.status")), format!("{status:?}\n")).unwrap();
    (status, fs::read(stdout).unwrap(), fs::read(stderr).unwrap())
}
fn okay(dir: &Path, label: &str, cmd: &mut Command) -> Vec<u8> {
    let (status, out, err) = command(dir, label, cmd);
    assert!(
        status.success(),
        "{label}: {status:?} {}",
        String::from_utf8_lossy(&err)
    );
    out
}
fn caller(mutable: bool) -> String {
    let pair = "{ ptr, i64 }";
    let mut text = "target triple = \"x86_64-unknown-linux-gnu\"\n".to_owned();
    for index in 0..7 {
        let n = index + 1;
        let values = (0..n)
            .map(|j| format!("i64 {}", if j == 0 { n } else { 91 }))
            .collect::<Vec<_>>()
            .join(", ");
        text.push_str(&format!(
            "@a{index} = private constant [{n} x i64] [{values}], align 8\n"
        ));
    }
    let values = (100..116)
        .map(|n| format!("i64 {n}"))
        .collect::<Vec<_>>()
        .join(", ");
    text.push_str(&format!(
        "@backing = global [16 x i64] [{values}], align 8\n"
    ));
    let types = std::iter::repeat_n(pair, 8)
        .chain(["i64"])
        .collect::<Vec<_>>()
        .join(", ");
    text.push_str(&format!(
        "declare {pair} @zil.fn.0({types})\ndefine i32 @main() {{\nentry:\n"
    ));
    let args = (0..7)
        .map(|i| format!("{pair} {{ ptr @a{i}, i64 {} }}", i + 1))
        .chain([format!("{pair} {{ ptr @backing, i64 16 }}"), "i64 1".into()])
        .collect::<Vec<_>>()
        .join(", ");
    text.push_str(&format!("  %out = call {pair} @zil.fn.0({args})\n  %data = extractvalue {pair} %out, 0\n  %len = extractvalue {pair} %out, 1\n  %first = load i64, ptr %data, align 8\n  %lastptr = getelementptr i64, ptr %data, i64 8\n  %last = load i64, ptr %lastptr, align 8\n  %goodlen = icmp eq i64 %len, 9\n  %goodfirst = icmp eq i64 %first, {}\n  %goodlast = icmp eq i64 %last, 113\n  %good0 = and i1 %goodlen, %goodfirst\n  %good1 = and i1 %good0, %goodlast\n", if mutable {112} else {105}));
    if mutable {
        text.push_str("  store i64 222, ptr %data, align 8\n  %original = getelementptr [16 x i64], ptr @backing, i64 0, i64 5\n  %current = load i64, ptr %original, align 8\n  %aliasok = icmp eq i64 %current, 222\n  %good = and i1 %good1, %aliasok\n");
    } else {
        text.push_str("  %good = and i1 %good1, true\n");
    }
    text.push_str("  %code = select i1 %good, i32 0, i32 1\n  ret i32 %code\n}\n");
    text
}

#[test]
fn separate_objects_preserve_eight_fat_arguments_and_shared_or_mutable_result() {
    for mutable in [false, true] {
        let qualifier = if mutable { "&mut" } else { "&" };
        let modification = if mutable { "h[start]=h[start]+7;" } else { "" };
        // Distinct weights make even a permutation of intact argument pairs
        // observable; a commutative unweighted sum would miss that ABI error.
        let text = format!(
            "fn choose(a:&[i64],b:&[i64],c:&[i64],d:&[i64],e:&[i64],f:&[i64],g:&[i64],h:{qualifier}[i64],low:i64)->{qualifier}[i64]{{let start=a[0]+b[0]*2+c[0]*3+d[0]*4+e[0]*5+f[0]*6+g[0]*7+low-136;let count=a.len()+b.len()*2+c.len()*3+d.len()*4+e.len()*5+f.len()*6+g.len()*7;let high=h.len()-(count-138);{modification}return {qualifier} h[start..high]}}"
        );
        for optimization in ["-O0", "-O2"] {
            let dir = directory("external-objects");
            let llvm = emit(&text, &dir, None);
            assert!(llvm.contains("define { ptr, i64 } @zil.fn.0("));
            assert!(!llvm.contains(" noalias "));
            fs::write(dir.join("caller.ll"), caller(mutable)).unwrap();
            okay(
                &dir,
                "callee-build",
                Command::new("clang")
                    .args([optimization, "-Wno-override-module", "-c"])
                    .arg(dir.join("callee.ll"))
                    .arg("-o")
                    .arg(dir.join("callee.o")),
            );
            okay(
                &dir,
                "caller-build",
                Command::new("clang")
                    .args([optimization, "-Wno-override-module", "-c"])
                    .arg(dir.join("caller.ll"))
                    .arg("-o")
                    .arg(dir.join("caller.o")),
            );
            okay(
                &dir,
                "link",
                Command::new("clang")
                    .arg(dir.join("callee.o"))
                    .arg(dir.join("caller.o"))
                    .arg("-o")
                    .arg(dir.join("program")),
            );
            let output = okay(&dir, "execute", &mut Command::new(dir.join("program")));
            assert!(output.is_empty());
        }
    }
}

#[test]
fn enum_boolean_and_nested_copy_stride_match_real_machine_values() {
    let cases = [
        (
            "enum E{Small(u8),Wide(i64)}fn score(x:E)->i64{match x{E.Small(n)=>{return i64(n)},E.Wide(n)=>{return n}}}fn mutate(p:&mut [E]){p[1]=E.Wide(5000000000)}fn value()->i64{var a=[E.Small(7u8),E.Small(8u8),E.Wide(11)];mutate(&mut a[..]);let p=&a[1..];return score(p[0])+score(p[1])+p.len()}",
            "5000000013\n",
        ),
        (
            "fn flip(p:&mut [bool]){var i:i64=0;while i<p.len(){p[i]=!p[i];i+=1}}fn value()->i64{var a=[true,false,false,true];flip(&mut a[1..]);let p=&a[..];var result:i64=0;var i:i64=0;while i<p.len(){if p[i]{result+=i+1}i+=1}return result}",
            "6\n",
        ),
        (
            "struct R{a:u8;flag:bool;wide:i64}fn value()->i64{var a:[[R;2];2]=[[R{a:1u8,flag:true,wide:2},R{a:3u8,flag:false,wide:4}],[R{a:5u8,flag:true,wide:6},R{a:7u8,flag:true,wide:8}]];let p=&mut a[1..];p[0][1].wide=5000000000;let q=&p[..];return q[0][1].wide+i64(q[0][0].a)+q.len()}",
            "5000000006\n",
        ),
    ];
    for (text, expected) in cases {
        for optimization in ["-O0", "-O2"] {
            let dir = directory("copy-stride");
            emit(text, &dir, Some("value"));
            okay(
                &dir,
                "build",
                Command::new("clang")
                    .args([optimization, "-Wno-override-module"])
                    .arg(dir.join("callee.ll"))
                    .arg("-o")
                    .arg(dir.join("program")),
            );
            let actual = okay(&dir, "execute", &mut Command::new(dir.join("program")));
            assert_eq!(actual, expected.as_bytes());
        }
    }
}
