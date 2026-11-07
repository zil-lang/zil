import std.i64_hash_map
import std.i64_hash_set
fn main()->i32 {
    match i64_hash_set.with_capacity(1) {
        .err(_)=>{return 1},
        .ok(created)=>{
            var set:i64_hash_set.I64HashSet=created
            i64_hash_map.insert(&mut set,1,42)
            return 0
        }
    }
}
