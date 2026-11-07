import std.i64_hash_set
fn main()->i32 {
    match i64_hash_set.with_capacity(3) {
        .err(_)=>{return 1},
        .ok(created)=>{
            var set:i64_hash_set.I64HashSet=created
            match i64_hash_set.insert(&mut set,1) {.err(_)=>{return 2},.ok(added)=>{if !added{return 3}}}
            match i64_hash_set.insert(&mut set,8) {.err(_)=>{return 4},.ok(added)=>{if !added{return 5}}}
            match i64_hash_set.insert(&mut set,-6) {.err(_)=>{return 6},.ok(added)=>{if !added{return 7}}}
            match i64_hash_set.insert(&mut set,1) {.err(_)=>{return 8},.ok(added)=>{if added{return 9}}}
            if !i64_hash_set.contains(&set,1)||!i64_hash_set.contains(&set,8)||!i64_hash_set.contains(&set,-6)||i64_hash_set.len(&set)!=3 {return 10}
            match i64_hash_set.insert(&mut set,15) {.ok(_)=>{return 11},.err(_)=>{}}
            match i64_hash_set.remove(&mut set,8) {.err(_)=>{return 12},.ok(removed)=>{if !removed{return 13}}}
            if i64_hash_set.contains(&set,8)||!i64_hash_set.contains(&set,1)||!i64_hash_set.contains(&set,-6) {return 14}
            match i64_hash_set.insert(&mut set,15) {.err(_)=>{return 15},.ok(added)=>{if !added{return 16}}}
            if !i64_hash_set.contains(&set,15)||i64_hash_set.len(&set)!=3 {return 17}
            return 0
        }
    }
}
