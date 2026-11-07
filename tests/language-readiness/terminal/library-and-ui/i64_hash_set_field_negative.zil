import std.i64_hash_set
fn main()->i32 {
    match i64_hash_set.with_capacity(1) {
        .err(_)=>{return 1},
        .ok(set)=>{return i32(set.state.limit)}
    }
}
