import std.pod_hash_map
struct Key { id:i64; domain:i64 }
struct Value { payload:i64; marked:bool }
fn records()->pod_hash_map.Error!i32 {
    let equal=(a:Key,b:Key)=>a.id==b.id && a.domain==b.domain
    let hash=(key:Key)=>key.id-key.id-5
    let created=pod_hash_map.with_capacity::<Key,Value>(1)?
    var map:pod_hash_map.PodHashMap<Key,Value>=created
    match pod_hash_map.insert(&mut map,Key{id:0,domain:0},Value{payload:7,marked:true},&equal,&hash) {.err(_)=>{return 1},.ok(_)=>{}}
    match pod_hash_map.insert_growing(&mut map,Key{id:0,domain:0},Value{payload:9,marked:false},&equal,&hash) {
        .err(_)=>{return 2},
        .ok(old)=>{match old {Option.None()=>{return 3},Option.Some(value)=>{if value.payload!=7 || !value.marked{return 4}}}}
    }
    if pod_hash_map.capacity(&map)!=1 {return 5}
    var at:i64=1
    while at<20 {
        let key=Key{id:at,domain:at%3}
        let value=Value{payload:at*7,marked:at%2==0}
        match pod_hash_map.insert_growing(&mut map,key,value,&equal,&hash) {
            .err(_)=>{return 6},
            .ok(old)=>{match old {Option.None()=>{},Option.Some(_)=>{return 7}}}
        }
        if key.id!=at || value.payload!=at*7 {return 8}
        at+=1
    }
    if pod_hash_map.len(&map)!=20 || pod_hash_map.capacity(&map)<20 {return 9}
    match pod_hash_map.reserve(&mut map,80,&equal,&hash) {.err(_)=>{return 10},.ok(_)=>{}}
    if pod_hash_map.capacity(&map)!=80 {return 11}
    match pod_hash_map.reserve(&mut map,10,&equal,&hash) {.err(_)=>{return 12},.ok(_)=>{}}
    if pod_hash_map.capacity(&map)!=80 {return 13}
    at=0
    while at<20 {
        match pod_hash_map.at(&map,at) {Option.None()=>{return 14},Option.Some(pair)=>{if pair.key.id!=at || pair.key.domain!=at%3{return 15}}}
        match pod_hash_map.get(&map,Key{id:at,domain:at%3},&equal,&hash) {
            Option.None()=>{return 16},
            Option.Some(value)=>{
                if at==0 {if value.payload!=9 || value.marked{return 17}}
                else {if value.payload!=at*7 || value.marked!=(at%2==0){return 18}}
            }
        }
        at+=1
    }
    match pod_hash_map.remove(&mut map,Key{id:5,domain:2},&equal,&hash) {.err(_)=>{return 19},.ok(old)=>{match old {Option.None()=>{return 20},Option.Some(value)=>{if value.payload!=35{return 21}}}}}
    match pod_hash_map.remove(&mut map,Key{id:19,domain:1},&equal,&hash) {.err(_)=>{return 22},.ok(_)=>{}}
    match pod_hash_map.remove(&mut map,Key{id:0,domain:0},&equal,&hash) {.err(_)=>{return 23},.ok(_)=>{}}
    if pod_hash_map.len(&map)!=17 {return 24}
    at=0
    while at<20 {
        match pod_hash_map.get(&map,Key{id:at,domain:at%3},&equal,&hash) {
            Option.None()=>{if at!=0 && at!=5 && at!=19{return 25}},
            Option.Some(value)=>{if at==0 || at==5 || at==19 || value.payload!=at*7{return 26}}
        }
        at+=1
    }
    at=100
    while at<103 {
        match pod_hash_map.insert_growing(&mut map,Key{id:at,domain:1},Value{payload:at,marked:true},&equal,&hash) {.err(_)=>{return 27},.ok(_)=>{}}
        at+=1
    }
    if pod_hash_map.clear(&mut map)!=20 || pod_hash_map.len(&map)!=0 || pod_hash_map.capacity(&map)!=80 {return 28}
    match pod_hash_map.get(&map,Key{id:101,domain:1},&equal,&hash) {Option.None()=>{},Option.Some(_)=>{return 29}}
    match pod_hash_map.insert(&mut map,Key{id:4,domain:2},Value{payload:33,marked:true},&equal,&hash) {.err(_)=>{return 30},.ok(_)=>{}}
    match pod_hash_map.get(&map,Key{id:4,domain:2},&equal,&hash) {Option.None()=>{return 31},Option.Some(value)=>{if value.payload!=33{return 32}}}
    return 0
}
fn scalar()->pod_hash_map.Error!i32 {
    let equal=(a:i64,b:i64)=>a==b
    let hash=(key:i64)=>key-key-1
    let created=pod_hash_map.with_capacity::<i64,bool>(0)?
    var map:pod_hash_map.PodHashMap<i64,bool>=created
    match pod_hash_map.insert(&mut map,1,true,&equal,&hash) {.ok(_)=>{return 41},.err(error)=>{match error {pod_hash_map.Error.Capacity()=>{},_=>{return 42}}}}
    if pod_hash_map.len(&map)!=0 || pod_hash_map.capacity(&map)!=0 {return 43}
    var key:i64=1
    while key<18 {
        pod_hash_map.insert_growing(&mut map,key,key%2==0,&equal,&hash)?
        key+=1
    }
    let capacity=pod_hash_map.capacity(&map)
    match pod_hash_map.reserve(&mut map,-1,&equal,&hash) {.ok(_)=>{return 44},.err(error)=>{match error {pod_hash_map.Error.Capacity()=>{},_=>{return 45}}}}
    match pod_hash_map.reserve(&mut map,4611686018427387904,&equal,&hash) {.ok(_)=>{return 46},.err(error)=>{match error {pod_hash_map.Error.Capacity()=>{},_=>{return 47}}}}
    if pod_hash_map.capacity(&map)!=capacity || pod_hash_map.len(&map)!=17 {return 48}
    key=1
    while key<18 {
        match pod_hash_map.get(&map,key,&equal,&hash) {Option.None()=>{return 49},Option.Some(value)=>{if value!=(key%2==0){return 50}}}
        key+=1
    }
    return 0
}
fn main()->i32 {
    match records() {.err(_)=>{return 59},.ok(code)=>{if code!=0 {return code}}}
    match scalar() {.err(_)=>{return 60},.ok(code)=>{return code}}
}
