import std.i64_hash_map
// A set uses the same collision and deletion engine, storing the unit marker 1.
// The nominal owner keeps Map value-writing operations out of the Set API.
pub package I64HashSet { state:i64_hash_map.I64HashMap }
pub fn with_capacity(limit:i64)->i64_hash_map.Error!I64HashSet {
    let inner=i64_hash_map.with_capacity(limit)?
    return I64HashSet{state:inner}
}
pub fn len(set:&I64HashSet)->i64 {return i64_hash_map.len(&set.state)}
pub fn key_at(set:&I64HashSet,index:i64)->Option<i64> {
    match i64_hash_map.at(&set.state,index) {
        Option.None()=>{return Option.None},
        Option.Some(pair)=>{return Option.Some(pair.key)}
    }
}
pub fn contains(set:&I64HashSet,key:i64)->bool {
    match i64_hash_map.get(&set.state,key) {
        Option.None()=>{return false},
        Option.Some(_)=>{return true}
    }
}
pub fn insert(set:&mut I64HashSet,key:i64)->i64_hash_map.Error!bool {
    match i64_hash_map.insert(&mut set.state,key,1) {
        .err(error)=>{return error},
        .ok(old)=>{
            let prior:Option<i64>=old
            match prior {
                Option.None()=>{return true},
                Option.Some(_)=>{return false}
            }
        }
    }
}
pub fn remove(set:&mut I64HashSet,key:i64)->i64_hash_map.Error!bool {
    match i64_hash_map.remove(&mut set.state,key) {
        .err(error)=>{return error},
        .ok(old)=>{
            let prior:Option<i64>=old
            match prior {
                Option.None()=>{return false},
                Option.Some(_)=>{return true}
            }
        }
    }
}
