import std.pod_hash_map
struct Key { id:i64; group:i64 }
struct Value { bits:i64; flag:bool }
fn is_allocation(error:pod_hash_map.Error)->bool {
    match error {pod_hash_map.Error.Allocation()=>{return true},_=>{return false}}
}
fn main()->i32 {
    let equal=(a:Key,b:Key)=>a.id==b.id && a.group==b.group
    let hash=(key:Key)=>key.id-key.id-3
    match pod_hash_map.with_capacity::<Key,Value>(1) {
        .err(error)=>{if !is_allocation(error){return 1};return 0},
        .ok(created)=>{
            var map:pod_hash_map.PodHashMap<Key,Value>=created
            match pod_hash_map.insert(&mut map,Key{id:7,group:2},Value{bits:11,flag:true},&equal,&hash) {.err(_)=>{return 2},.ok(_)=>{}}
            match pod_hash_map.insert_growing(&mut map,Key{id:7,group:2},Value{bits:11,flag:true},&equal,&hash) {
                .err(_)=>{return 22},
                .ok(old)=>{match old {Option.None()=>{return 23},Option.Some(value)=>{if value.bits!=11 || !value.flag{return 24}}}}
            }
            if pod_hash_map.capacity(&map)!=1 {return 25}
            let incoming_key=Key{id:8,group:4}
            let incoming_value=Value{bits:99,flag:false}
            match pod_hash_map.insert_growing(&mut map,incoming_key,incoming_value,&equal,&hash) {
                .err(error)=>{
                    if !is_allocation(error) || pod_hash_map.len(&map)!=1 || pod_hash_map.capacity(&map)!=1 {return 3}
                    if incoming_key.id!=8 || incoming_key.group!=4 || incoming_value.bits!=99 || incoming_value.flag {return 4}
                    match pod_hash_map.get(&map,Key{id:7,group:2},&equal,&hash) {Option.None()=>{return 5},Option.Some(value)=>{if value.bits!=11 || !value.flag{return 6}}}
                    match pod_hash_map.get(&map,incoming_key,&equal,&hash) {Option.None()=>{},Option.Some(_)=>{return 7}}
                    match pod_hash_map.insert_growing(&mut map,incoming_key,incoming_value,&equal,&hash) {.err(_)=>{return 8},.ok(_)=>{}}
                },
                .ok(old)=>{match old {Option.None()=>{},Option.Some(_)=>{return 9}}}
            }
            if pod_hash_map.len(&map)!=2 || pod_hash_map.capacity(&map)!=2 {return 10}
            match pod_hash_map.reserve(&mut map,8,&equal,&hash) {
                .err(error)=>{
                    if !is_allocation(error) || pod_hash_map.len(&map)!=2 || pod_hash_map.capacity(&map)!=2 {return 11}
                    match pod_hash_map.get(&map,incoming_key,&equal,&hash) {Option.None()=>{return 12},Option.Some(value)=>{if value.bits!=99 || value.flag{return 13}}}
                    match pod_hash_map.get(&map,Key{id:7,group:2},&equal,&hash) {Option.None()=>{return 14},Option.Some(value)=>{if value.bits!=11 || !value.flag{return 15}}}
                    match pod_hash_map.reserve(&mut map,8,&equal,&hash) {.err(_)=>{return 16},.ok(_)=>{}}
                },
                .ok(_)=>{}
            }
            if pod_hash_map.len(&map)!=2 || pod_hash_map.capacity(&map)!=8 {return 17}
            match pod_hash_map.get(&map,incoming_key,&equal,&hash) {Option.None()=>{return 18},Option.Some(value)=>{if value.bits!=99 || value.flag{return 19}}}
            match pod_hash_map.get(&map,Key{id:7,group:2},&equal,&hash) {Option.None()=>{return 20},Option.Some(value)=>{if value.bits!=11 || !value.flag{return 21}}}
            return 0
        }
    }
}
