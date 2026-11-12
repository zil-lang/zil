use std::{fs, process::Command, sync::atomic::{AtomicU64, Ordering}};
use zil_base::ResultCategory;
use zil_driver::{compile_path, CompileOptions, NativeBackend, Optimization, Stage};
static NEXT: AtomicU64 = AtomicU64::new(0);
const CONSUMER: &str = r#"
import std.owning_hash_map
import std.owning_hash_set
import std.owning_entries
import std.owning_map
package Key { manages bytes:ByteBuffer; id:i64 }
package Value { manages bytes:ByteBuffer; id:i64 }
impl owning_map.KeyEq for Key {fn same(self:&Self,other:&Self)->bool{return self.id==other.id}}
impl owning_hash_map.KeyHash for Key {fn hash(self:&Self)->i64{return -9223372036854775808}}
fn key(id:i64)->Key {return Key{bytes:bytes_new(),id:id}}
fn value(id:i64)->Value {return Value{bytes:bytes_new(),id:id}}
fn main()->i32 {
    var map=owning_hash_map.new::<Key,Value>()
    var at:i64=0
    while at<20 {
        let entry=owning_entries.Entry::<Key,Value>{key:key(at),value:value(at+10)}
        match owning_hash_map.insert::<Key,Value>(&mut map,entry) {.err(_)=>{return 1},.ok(replaced)=>{if replaced{return 2}}}
        at+=1
    }
    if owning_hash_map.len::<Key,Value>(&map)!=20 || owning_hash_map.capacity::<Key,Value>(&map)!=32 {return 3}
    let query=key(7)
    match owning_hash_map.get::<Key,Value>(&map,&query) {Option.None()=>{return 4},Option.Some(found)=>{if found.id!=17{return 5}}}
    let replacement=owning_entries.Entry::<Key,Value>{key:key(7),value:value(99)}
    match owning_hash_map.insert::<Key,Value>(&mut map,replacement) {.err(_)=>{return 6},.ok(replaced)=>{if !replaced{return 7}}}
    match owning_hash_map.take::<Key,Value>(&mut map,&query) {.err(_)=>{return 8},.ok(old)=>{if old.key.id!=7 || old.value.id!=99{return 9}}}
    at=0
    while at<20 {
        let query=key(at)
        if owning_hash_map.contains::<Key,Value>(&map,&query)!=(at!=7) {return 10}
        at+=1
    }
    if owning_hash_map.clear::<Key,Value>(&mut map)!=19 {return 11}
    var set=owning_hash_set.new::<Key>()
    at=0
    while at<20 {
        match owning_hash_set.insert::<Key>(&mut set,key(at)) {.err(_)=>{return 12},.ok(inserted)=>{if !inserted{return 13}}}
        at+=1
    }
    match owning_hash_set.insert::<Key>(&mut set,key(7)) {.err(_)=>{return 14},.ok(inserted)=>{if inserted{return 15}}}
    match owning_hash_set.take::<Key>(&mut set,&query) {.err(_)=>{return 16},.ok(old)=>{if old.id!=7{return 17}}}
    match owning_hash_set.key_at::<Key>(&set,0) {Option.None()=>{return 18},Option.Some(first)=>{if first.id!=0{return 19}}}
    if owning_hash_set.clear::<Key>(&mut set)!=19 {return 20}
    return 0
}
"#;
fn run(source: &str, name: &str) {
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature="llvm-api") {continue;}
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir=std::env::temp_dir().join(format!("zil-owning-hash-{name}-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
            fs::create_dir_all(&dir).unwrap();
            let path=dir.join("main.zil"); fs::write(&path,source).unwrap();
            let report=compile_path(&path,&CompileOptions{stop_after:Stage::Build,backend,optimization,output:Some(dir.join("main")),..CompileOptions::default()});
            assert_eq!(report.status,ResultCategory::Success,"{}",report.to_json());
            let output=Command::new(&report.artifacts["executable"]).output().unwrap();
            assert_eq!(output.status.code(),Some(0),"{backend:?}/{optimization:?}: {output:?}");
            fs::remove_dir_all(dir).unwrap();
        }
    }
}
#[test] fn owning_collision_growth_replacement_take_set_and_reuse() {run(CONSUMER,"collections");}
#[test] fn full_width_unsigned_decimal_roundtrip_and_range() {run(r#"
import std.int
fn main()->i32 {
    let formatted=int.format_u64(18446744073709551615u64)
    if formatted.len!=20 || formatted.bytes[0]!=49u8 || formatted.bytes[19]!=53u8 {return 1}
    match int.parse_u64(&formatted.bytes) {.err(_)=>{return 2},.ok(value)=>{if value!=18446744073709551615u64{return 3}}}
    match int.parse_u64("18446744073709551616".bytes()) {.err(error)=>{match error{int.ParseError.Overflow()=>{},_=>{return 4}}},.ok(_)=>{return 5}}
    match int.parse_u64("-1".bytes()) {.err(_)=>{},.ok(_)=>{return 6}}
    let zero=int.format_u64(0u64)
    if zero.len!=1 || zero.bytes[0]!=48u8 {return 7}
    return 0
}
"#,"unsigned");}
