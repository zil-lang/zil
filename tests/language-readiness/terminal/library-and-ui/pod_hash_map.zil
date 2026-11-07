import std.pod_hash_map
struct Key { n:i32 }
struct Value { n:i32 }
fn record()->i32 {
    let equal=(a:Key,b:Key)=>a.n==b.n
    let hash=(a:Key)=>i64(a.n)
    match pod_hash_map.with_capacity::<Key,Value>(3) {
        .err(_)=>{return 1},
        .ok(created)=>{
            var map:pod_hash_map.PodHashMap<Key,Value>=created
            match pod_hash_map.insert(&mut map,Key{n:7},Value{n:12},&equal,&hash) {.err(_)=>{return 2},.ok(_)=>{}}
            match pod_hash_map.get(&map,Key{n:7},&equal,&hash) {Option.None()=>{return 3},Option.Some(value)=>{if value.n!=12{return 4}}}
            match pod_hash_map.remove(&mut map,Key{n:7},&equal,&hash) {.err(_)=>{return 5},.ok(old)=>{let found:Option<Value>=old;match found {Option.None()=>{return 6},Option.Some(value)=>{if value.n!=12{return 7}}}}}
            return i32(pod_hash_map.len::<Key,Value>(&map))
        }
    }
}
fn primitive()->i32 {
    let equal=(a:i64,b:i64)=>a==b
    let hash=(a:i64)=>a
    match pod_hash_map.with_capacity::<i64,bool>(3) {
        .err(_)=>{return 11},
        .ok(created)=>{
            var map:pod_hash_map.PodHashMap<i64,bool>=created
            match pod_hash_map.insert(&mut map,-7,true,&equal,&hash) {.err(_)=>{return 12},.ok(_)=>{}}
            match pod_hash_map.get(&map,-7,&equal,&hash) {Option.None()=>{return 13},Option.Some(value)=>{if !value{return 14}}}
            match pod_hash_map.remove(&mut map,-7,&equal,&hash) {.err(_)=>{return 15},.ok(old)=>{let found:Option<bool>=old;match found {Option.None()=>{return 16},Option.Some(value)=>{if !value{return 17}}}}}
            return i32(pod_hash_map.len::<i64,bool>(&map))
        }
    }
}
fn main()->i32 {let first=record();if first!=0{return first}return primitive()}
