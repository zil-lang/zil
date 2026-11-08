import std.pod_hash_map
import std.pod_hash_set
struct Key { id:i64; group:i64 }
fn is_allocation(error:pod_hash_map.Error)->bool {
    match error {pod_hash_map.Error.Allocation()=>{return true},_=>{return false}}
}
fn main()->i32 {
    let equal=(a:Key,b:Key)=>a.id==b.id && a.group==b.group
    let hash=(key:Key)=>key.id-key.id-3
    match pod_hash_set.with_capacity::<Key>(1) {
        .err(error)=>{if !is_allocation(error){return 1};return 0},
        .ok(created)=>{
            var set:pod_hash_set.PodHashSet<Key>=created
            match pod_hash_set.insert(&mut set,Key{id:7,group:2},&equal,&hash) {.err(_)=>{return 2},.ok(added)=>{if !added{return 3}}}
            match pod_hash_set.insert_growing(&mut set,Key{id:7,group:2},&equal,&hash) {.err(_)=>{return 4},.ok(added)=>{if added || pod_hash_set.capacity(&set)!=1{return 5}}}
            let incoming_key=Key{id:8,group:4}
            match pod_hash_set.insert_growing(&mut set,incoming_key,&equal,&hash) {
                .err(error)=>{
                    if !is_allocation(error) || pod_hash_set.len(&set)!=1 || pod_hash_set.capacity(&set)!=1 {return 6}
                    if incoming_key.id!=8 || incoming_key.group!=4 {return 7}
                    if !pod_hash_set.contains(&set,Key{id:7,group:2},&equal,&hash) || pod_hash_set.contains(&set,incoming_key,&equal,&hash) {return 8}
                    match pod_hash_set.insert_growing(&mut set,incoming_key,&equal,&hash) {.err(_)=>{return 9},.ok(added)=>{if !added{return 10}}}
                },
                .ok(added)=>{if !added{return 11}}
            }
            if pod_hash_set.len(&set)!=2 || pod_hash_set.capacity(&set)!=2 {return 12}
            match pod_hash_set.reserve(&mut set,8,&equal,&hash) {
                .err(error)=>{
                    if !is_allocation(error) || pod_hash_set.len(&set)!=2 || pod_hash_set.capacity(&set)!=2 {return 13}
                    if !pod_hash_set.contains(&set,incoming_key,&equal,&hash) || !pod_hash_set.contains(&set,Key{id:7,group:2},&equal,&hash) {return 14}
                    match pod_hash_set.reserve(&mut set,8,&equal,&hash) {.err(_)=>{return 15},.ok(_)=>{}}
                },
                .ok(_)=>{}
            }
            if pod_hash_set.len(&set)!=2 || pod_hash_set.capacity(&set)!=8 {return 16}
            if !pod_hash_set.contains(&set,incoming_key,&equal,&hash) || !pod_hash_set.contains(&set,Key{id:7,group:2},&equal,&hash) {return 17}
            return 0
        }
    }
}
