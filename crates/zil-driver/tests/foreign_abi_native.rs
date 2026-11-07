//! Source, independent Verify, LLVM and native C ABI evidence. The C fixture is
//! a test oracle, not a window host or a replacement compiler safety checker.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_backend_llvm::{EmitOptions, EntryPoint};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Optimization, Stage, compile_source};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf, bool);
impl Temp {
    fn new() -> Self {
        let keep = std::env::var_os("ZIL_TEST_REPORT_DIR").is_some();
        let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = base.join(format!(
            "zil-native087-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path, keep)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn input(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "native087.zil", text)
}
fn build(text: &str, optimization: Optimization) -> Temp {
    let temp = Temp::new();
    fs::write(temp.0.join("source.zil"), text).unwrap();
    let report = compile_source(
        &input(text),
        &CompileOptions {
            stop_after: Stage::Build,
            entry: Some("value".into()),
            output: Some(temp.0.join("program")),
            emit_dir: Some(temp.0.join("phases")),
            optimization,
            ..CompileOptions::default()
        },
    );
    fs::write(temp.0.join("compile.json"), report.to_json()).unwrap();
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    temp
}
fn run(temp: &Temp) -> std::process::Output {
    let output = Command::new(temp.0.join("program")).output().unwrap();
    fs::write(temp.0.join("stdout"), &output.stdout).unwrap();
    fs::write(temp.0.join("stderr"), &output.stderr).unwrap();
    fs::write(temp.0.join("status"), format!("{:?}", output.status)).unwrap();
    output
}

#[test]
fn u16_source_ct_aggregates_unsigned_arithmetic_and_conversion_execute_at_o0_o2() {
    let cases = [
        ("fn value()->u16{return #(65535u16)}", "65535\n"),
        (
            "fn id<T>(x:T)->T=>x;fn value()->u16{return id(65535u16)}",
            "65535\n",
        ),
        (
            "fn value()->u16{let a=[3u16,60000u16];return a[1u16]}",
            "60000\n",
        ),
        ("fn value()->u16{return 0u16-%1u16}", "65535\n"),
        ("fn value()->u16{return 65535u16+%2u16}", "1\n"),
        ("fn value()->u16{return 0x8000u16>>15u16}", "1\n"),
        ("fn value()->u16{return 65535u16/3u16}", "21845\n"),
        ("fn value()->u16{return 65535u16%256u16}", "255\n"),
        ("fn value()->bool{return 65535u16>256u16}", "1\n"),
        ("fn value()->i64{return i64(u16(65535))}", "65535\n"),
        ("fn value()->u8{return u8(u16(255))}", "255\n"),
        (
            "fn helper(x:u16)->u16{return x+%1u16}fn value()->u16{return #(helper(65535u16))}",
            "0\n",
        ),
        (
            "extern \"C\" fn htons(x:u16)->u16;fn value()->u16{unsafe{return htons(0x1234u16)}}",
            "13330\n",
        ),
        ("fn value()->bool{let h=c_null();return h==c_null()}", "1\n"),
    ];
    for level in [Optimization::O0, Optimization::O2] {
        for (source, expected) in cases {
            let temp = build(source, level);
            let output = run(&temp);
            assert!(output.status.success(), "{source}: {output:?}");
            assert_eq!(output.stdout, expected.as_bytes(), "{source}");
            assert!(output.stderr.is_empty(), "{source}: {output:?}");
        }
    }
}

#[cfg(unix)]
#[test]
fn u16_checked_failures_trap_before_output_at_o0_o2() {
    use std::os::unix::process::ExitStatusExt;
    for level in [Optimization::O0, Optimization::O2] {
        for expression in [
            "65535u16+1u16",
            "0u16-1u16",
            "256u16*256u16",
            "1u16/0u16",
            "1u16%0u16",
            "1u16<<16u16",
            "u16(-1)",
            "u16(65536)",
            "u8(256u16)",
        ] {
            let temp = build(
                &format!("fn value()->i32{{return i32({expression})}}"),
                level,
            );
            let output = run(&temp);
            assert_eq!(
                output.status.signal(),
                Some(4),
                "expected LLVM ud2: {expression}: {output:?}"
            );
            assert!(output.stdout.is_empty());
        }
    }
}

const ABI_SOURCE: &str = r#"
struct Shared { byte:u8; word:u16; number:i32; handle:c_handle; text:c_wstr;
    callback:extern "C" fn(c_handle,i32,i64)->i64 }
struct Mutable { byte:u8; word:u16; number:i32; handle:c_handle }
extern "C" fn token()->c_handle;
extern "C" fn echo16(x:u16)->u16;
extern "C" fn echo8(x:u8)->u8;
extern "C" fn inspect(value:&Shared)->i32
contract{provides(noescape(value),static_callbacks());effects(ffi);};
extern "C" fn after()->i32 contract{provides(static_callbacks());effects(ffi);};
extern "C" fn mutate(value:&mut Mutable)->i32 contract{provides(noescape(value));effects(ffi);};
fn callback(h:c_handle,a:i32,b:i64)->i64{if h==c_null(){return -90}return i64(a)+b}
fn main()->i32{
    var h=c_null(); unsafe{h=token();if echo16(60000u16)!=60000u16{return 1}
        if echo8(255u8)!=255u8{return 2}}
    if true {let shared=Shared{byte:255u8,word:60000u16,number:-7,handle:h,
        text:c_wstr("Z中😀"),callback:c_callback(callback)}
        unsafe{if inspect(&shared)!=0{return 3}}}
    var mutable=Mutable{byte:0u8,word:0u16,number:0,handle:c_null()}
    unsafe{if after()!=18{return 4}if mutate(&mut mutable)!=0{return 5}}
    if mutable.byte!=255u8||mutable.word!=65535u16||mutable.number != -123||mutable.handle!=h{return 6}
    return 0
}
"#;
const ABI_C: &str = r#"
typedef unsigned char U8; typedef unsigned short U16; typedef long long I64;
typedef I64 (*Callback)(void*,int,I64);
struct Shared { U8 byte; U16 word; int number; void *handle; const U16 *text; Callback callback; };
struct Mutable { U8 byte; U16 word; int number; void *handle; };
_Static_assert(sizeof(struct Shared)==32,"Shared C size");
_Static_assert(_Alignof(struct Shared)==8,"Shared C alignment");
_Static_assert(__builtin_offsetof(struct Shared,word)==2,"word padding");
_Static_assert(__builtin_offsetof(struct Shared,text)==16,"static text offset");
_Static_assert(__builtin_offsetof(struct Shared,callback)==24,"callback offset");
_Static_assert(sizeof(struct Mutable)==16,"Mutable C size");
static int identity; static const U16 *retained_text; static Callback retained_callback;
void *token(void){return &identity;}
U16 echo16(U16 x){return x;} U8 echo8(U8 x){return x;}
int inspect(const struct Shared *p){
    const U16 expected[5]={0x5a,0x4e2d,0xd83d,0xde00,0};
    if(p->byte!=255||p->word!=60000||p->number != -7||p->handle!=token())return 1;
    for(int i=0;i<5;i++)if(p->text[i]!=expected[i])return 2;
    retained_text=p->text; retained_callback=p->callback;
    return (p->callback(p->handle,7,11)==18)?0:3;
}
int after(void){if(retained_text[4]!=0||retained_text[1]!=0x4e2d)return -1;
    return (int)retained_callback(token(),7,11);}
int mutate(struct Mutable *p){p->byte=255;p->word=65535;p->number=-123;p->handle=token();return 0;}
"#;

fn emit_abi(target: &TargetProfile) -> String {
    let source = input(ABI_SOURCE);
    let limits = Limits::default();
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let typed = zil_types::check(&core, target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let verified = zil_verify::verify_module(typed, plan, target, &limits).unwrap();
    let lir = zil_lower::lower(&verified).unwrap();
    zil_backend_llvm::emit_with_options(
        &verified,
        &lir,
        &EmitOptions {
            entry: EntryPoint::Main,
        },
    )
    .unwrap()
}

#[test]
fn real_c_observes_record_offsets_narrow_values_and_static_retention_at_o0_o2() {
    let temp = Temp::new();
    let llvm = emit_abi(&TargetProfile::host_llvm());
    assert!(llvm.contains("declare zeroext i16 @echo16(i16 zeroext)"));
    assert!(llvm.contains("declare zeroext i8 @echo8(i8 zeroext)"));
    assert!(llvm.contains(
        "private unnamed_addr constant [5 x i16] [i16 90, i16 20013, i16 55357, i16 56832, i16 0]"
    ));
    assert!(!llvm.contains(" noalias "));
    assert!(!llvm.contains(" nocapture "));
    fs::write(temp.0.join("module.ll"), &llvm).unwrap();
    fs::write(temp.0.join("oracle.c"), ABI_C).unwrap();
    for opt in ["-O0", "-O2"] {
        let mut command =
            Command::new(std::env::var_os("ZIL_CLANG").unwrap_or_else(|| "clang".into()));
        command
            .args(["--target=x86_64-unknown-linux-gnu", opt, "-Werror"])
            .arg(temp.0.join("module.ll"))
            .arg(temp.0.join("oracle.c"))
            .arg("-o")
            .arg(temp.0.join("program"));
        let result = command.output().unwrap();
        fs::write(
            temp.0.join(format!("build-{opt}.txt")),
            format!("{command:?}\n{result:?}"),
        )
        .unwrap();
        assert!(result.status.success(), "{result:?}");
        let output = run(&temp);
        assert!(output.status.success(), "{output:?}");
        assert!(output.stdout.is_empty());
    }
    let windows = emit_abi(&TargetProfile::windows_x64_llvm());
    assert!(windows.contains("target triple = \"x86_64-pc-windows-msvc\""));
    assert!(windows.contains("declare i16 @echo16(i16)"));
    assert!(windows.contains("declare i8 @echo8(i8)"));
    assert!(windows.contains("define void @zil.pe.entry()"));
    assert!(!windows.contains("zeroext"));
    fs::write(temp.0.join("module-win64.ll"), windows).unwrap();
}
