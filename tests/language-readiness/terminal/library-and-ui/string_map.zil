import std.string
import std.string_map

fn main()->i32 {
    var map=string_map.new()
    let key=string.from_utf8("猫",16)
    match key {
        .err(_)=>{return 1},
        .ok(text)=>{
            match string_map.insert(&mut map,text,7) {
                .err(_)=>{return 2},
                .ok(_)=>{}
            }
        }
    }
    let query=string.from_utf8("猫",16)
    match query {
        .err(_)=>{return 3},
        .ok(text)=>{
            match string_map.get(&map,&text) {
                Option.None()=>{return 4},
                Option.Some(value)=>{if value!=7 {return 5}}
            }
            match string_map.remove(&mut map,&text) {
                .err(_)=>{return 6},
                .ok(found)=>{
                    let item:Option<i64>=found
                    match item {
                        Option.None()=>{return 7},
                        Option.Some(value)=>{if value!=7 {return 8}}
                    }
                }
            }
        }
    }
    if string_map.len(&map)!=0 {return 9}
    return 0
}
