import std.string
import std.string_map
extern "C" fn probe_fail_next();

fn key(index:i32)->string.String {
    var raw=[u8(65+index)]
    match string.from_utf8(&raw[..],1) {
        .ok(value)=>{return value},
        .err(_)=>{return string.empty()}
    }
}
fn main()->i32 {
    var map=string_map.new()
    var at:i32=0
    while at<8 {
        match string_map.insert(&mut map,key(at),i64(at)) {
            .err(_)=>{return 1},
            .ok(result)=>{if result.replaced {return 2}}
        }
        at+=1
    }
    if string_map.len(&map)!=8 {return 3}
    let incoming=key(8)
    unsafe{probe_fail_next()}
    match string_map.insert(&mut map,incoming,108) {
        .ok(_)=>{return 4},
        .err(failed)=>{
            if string.byte_len(&failed.key)!=1 {return 5}
            let bytes=string.as_bytes(&failed.key)
            if bytes[0]!=73u8 {return 6}
            if string_map.len(&map)!=8 {return 7}
            match string_map.insert(&mut map,failed.key,108) {
                .err(_)=>{return 8},
                .ok(result)=>{if result.replaced {return 9}}
            }
        }
    }
    if string_map.len(&map)!=9 {return 10}
    return 0
}
