import std.pod_hash_map
import std.pod_hash_set
struct Key { id:i64; domain:i64 }
fn records()->pod_hash_map.Error!i32 {
    let equal=(a:Key,b:Key)=>a.id==b.id && a.domain==b.domain
    let hash=(key:Key)=>key.id-key.id-7
    let created=pod_hash_set.with_capacity::<Key>(1)?
    var set:pod_hash_set.PodHashSet<Key>=created
    if !pod_hash_set.insert(&mut set,Key{id:0,domain:1},&equal,&hash)? {return 1}
    if pod_hash_set.insert_growing(&mut set,Key{id:0,domain:1},&equal,&hash)? || pod_hash_set.capacity(&set)!=1 {return 2}
    match pod_hash_set.insert(&mut set,Key{id:1,domain:1},&equal,&hash) {.ok(_)=>{return 3},.err(error)=>{match error {pod_hash_map.Error.Capacity()=>{},_=>{return 4}}}}
    var at:i64=1
    while at<24 {
        let key=Key{id:at,domain:at%4}
        if !pod_hash_set.insert_growing(&mut set,key,&equal,&hash)? || key.id!=at {return 5}
        at+=1
    }
    if pod_hash_set.len(&set)!=24 || pod_hash_set.capacity(&set)<24 {return 6}
    pod_hash_set.reserve(&mut set,75,&equal,&hash)?
    at=0
    while at<24 {
        var domain=at%4
        if at==0 {domain=1}
        if !pod_hash_set.contains(&set,Key{id:at,domain:domain},&equal,&hash) {return 7}
        match pod_hash_set.key_at(&set,at) {Option.None()=>{return 8},Option.Some(key)=>{if key.id!=at{return 9}}}
        at+=1
    }
    if !pod_hash_set.remove(&mut set,Key{id:7,domain:3},&equal,&hash)? {return 10}
    if pod_hash_set.remove(&mut set,Key{id:7,domain:3},&equal,&hash)? {return 11}
    if pod_hash_set.contains(&set,Key{id:7,domain:3},&equal,&hash) || pod_hash_set.len(&set)!=23 {return 12}
    if !pod_hash_set.insert_growing(&mut set,Key{id:7,domain:3},&equal,&hash)? {return 13}
    if pod_hash_set.clear(&mut set)!=24 || pod_hash_set.len(&set)!=0 || pod_hash_set.capacity(&set)!=75 {return 14}
    if pod_hash_set.contains(&set,Key{id:0,domain:1},&equal,&hash) {return 15}
    if !pod_hash_set.insert(&mut set,Key{id:90,domain:2},&equal,&hash)? {return 16}
    return 0
}
fn scalars()->pod_hash_map.Error!i32 {
    let equal=(a:i64,b:i64)=>a==b
    let hash=(key:i64)=>key%3
    let created=pod_hash_set.with_capacity::<i64>(0)?
    var set:pod_hash_set.PodHashSet<i64>=created
    var key:i64=-12
    while key<12 {
        if !pod_hash_set.insert_growing(&mut set,key,&equal,&hash)? {return 21}
        key+=1
    }
    if pod_hash_set.len(&set)!=24 {return 22}
    key=-12
    while key<12 {
        if !pod_hash_set.contains(&set,key,&equal,&hash) {return 23}
        key+=1
    }
    let limit=pod_hash_set.capacity(&set)
    match pod_hash_set.reserve(&mut set,4611686018427387904,&equal,&hash) {.ok(_)=>{return 24},.err(error)=>{match error {pod_hash_map.Error.Capacity()=>{},_=>{return 25}}}}
    if pod_hash_set.capacity(&set)!=limit || pod_hash_set.len(&set)!=24 {return 26}
    return 0
}
fn main()->i32 {
    match records() {.err(_)=>{return 31},.ok(code)=>{if code!=0{return code}}}
    match scalars() {.err(_)=>{return 32},.ok(code)=>{return code}}
}
