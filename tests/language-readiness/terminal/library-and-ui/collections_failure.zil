import std.collections
extern "C" fn probe_fail_next();

fn main()->i32 {
    var map=collections.map_new()
    var key:i64=0
    while key<8 {
        match collections.map_insert(&mut map,key,key+100) {
            .ok(_)=>{},
            .err(_)=>{return 1}
        }
        key+=1
    }
    let capacity=collections.map_capacity(&map)
    unsafe{probe_fail_next()}
    match collections.map_insert(&mut map,8,108) {
        .ok(_)=>{return 2},
        .err(_)=>{}
    }
    if collections.map_len(&map)!=8 || collections.map_capacity(&map)!=capacity {return 3}
    key=0
    while key<8 {
        match collections.map_get(&map,key) {
            Option.None()=>{return 4},
            Option.Some(value)=>{if value!=key+100 {return 5}}
        }
        key+=1
    }
    match collections.map_get(&map,8) {
        Option.None()=>{},
        Option.Some(_)=>{return 6}
    }
    match collections.map_insert(&mut map,8,108) {
        .ok(result)=>{if result.replaced {return 7}},
        .err(_)=>{return 8}
    }
    if collections.map_len(&map)!=9 {return 9}
    return 0
}
