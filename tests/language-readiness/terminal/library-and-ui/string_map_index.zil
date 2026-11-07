import std.string
import std.string_map

fn inspect(map:&string_map.StringMap)->i32 {
    match string_map.key_clone_at(map,0,2) {
        .ok(_)=>{return 1},
        .err(error)=>{match error {string.Error.Capacity()=>{},_=>{return 2}}}
    }
    match string_map.key_clone_at(map,0,16) {
        .err(_)=>{return 3},
        .ok(key)=>{if string.byte_len(&key)!=3 {return 4}}
    }
    match string_map.value_at(map,0) {
        Option.None()=>{return 5},
        Option.Some(value)=>{if value!=7 {return 6}}
    }
    match string_map.key_clone_at(map,1,16) {
        .ok(_)=>{return 7},
        .err(error)=>{match error {string.Error.Range()=>{},_=>{return 8}}}
    }
    return 0
}
fn main()->i32 {
    var map=string_map.new()
    match string.from_utf8("猫",16) {
        .err(_)=>{return 9},
        .ok(key)=>{
            match string_map.insert(&mut map,key,7) {
                .err(_)=>{return 10},.ok(_)=>{}
            }
        }
    }
    return inspect(&map)
}
