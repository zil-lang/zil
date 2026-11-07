import std.collections

fn map_checks()->collections.Error!i32 {
    var map=collections.map_new()
    var key:i64=-9
    while key<10 {
        let result=collections.map_insert(&mut map,key,key*3)?
        if result.replaced {return 1}
        key+=1
    }
    if collections.map_len(&map)!=19 {return 2}
    let replaced=collections.map_insert(&mut map,0,42)?
    if !replaced.replaced || replaced.previous!=0 || collections.map_len(&map)!=19 {return 3}
    match collections.map_get(&map,0) {
        Option.None()=>{return 4},
        Option.Some(value)=>{if value!=42 {return 5}}
    }
    match collections.map_get(&map,99) {
        Option.None()=>{},
        Option.Some(_)=>{return 6}
    }
    let removed=collections.map_remove(&mut map,-9)?
    if !removed.found || removed.value!=-27 || collections.map_len(&map)!=18 {return 7}
    let missing=collections.map_remove(&mut map,-9)?
    if missing.found || collections.map_len(&map)!=18 {return 8}
    match collections.map_get(&map,9) {
        Option.None()=>{return 9},
        Option.Some(value)=>{if value!=27 {return 10}}
    }
    return 0
}

fn set_checks()->collections.Error!i32 {
    var set=collections.set_new()
    var key:i64=-9
    while key<10 {
        if !collections.set_insert(&mut set,key)? {return 11}
        key+=1
    }
    if collections.set_len(&set)!=19 || !collections.set_contains(&set,-9) {return 12}
    if collections.set_insert(&mut set,0)? || collections.set_len(&set)!=19 {return 13}
    if !collections.set_remove(&mut set,-9)? || collections.set_contains(&set,-9) {return 14}
    if collections.set_remove(&mut set,-9)? || !collections.set_contains(&set,9) {return 15}
    return 0
}

fn main()->collections.Error!i32 {
    let map=map_checks()?
    if map!=0 {return map}
    return set_checks()
}
