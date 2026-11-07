import std.string
import std.string_map
extern "C" fn probe_fail_next();

fn main()->i32 {
    var map=string_map.new()
    match string_map.insert(&mut map,string.empty(),1) {
        .err(_)=>{return 1},.ok(_)=>{}
    }
    match string.from_utf8("x",4) {
        .err(_)=>{return 2},
        .ok(key)=>{
            match string_map.insert(&mut map,key,2) {
                .err(_)=>{return 3},.ok(_)=>{}
            }
        }
    }
    let query=string.empty()
    unsafe{probe_fail_next()}
    match string_map.remove(&mut map,&query) {
        .ok(_)=>{return 4},.err(_)=>{}
    }
    if string_map.len(&map)!=2 {return 5}
    match string_map.get(&map,&query) {
        Option.None()=>{return 6},
        Option.Some(value)=>{if value!=1 {return 7}}
    }
    match string_map.remove(&mut map,&query) {
        .err(_)=>{return 8},
        .ok(found)=>{
            let old:Option<i64>=found
            match old {
                Option.None()=>{return 9},
                Option.Some(value)=>{if value!=1 {return 10}}
            }
        }
    }
    if string_map.len(&map)!=1 {return 11}
    return 0
}
