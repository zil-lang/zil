import std.pod_hash_map

struct Key { n:i32 }
impl pod_hash_map.HashKey for Key {
    fn hash()->i64 {return i64(.n)}
    fn equals(other:Key)->bool {return .n==other.n}
}
struct Value { n:i32 }

fn main()->i32 {
    match pod_hash_map.with_capacity::<Key,Value>(4) {
        .err(_)=>{return 1},
        .ok(value)=>{
            var map:pod_hash_map.PodHashMap<Key,Value>=value
            match pod_hash_map.insert::<Key,Value>(&mut map,Key{n:7},Value{n:12}) {
                .err(_)=>{return 2},
                .ok(_)=>{}
            }
            match pod_hash_map.get::<Key,Value>(&map,Key{n:7}) {
                Option.None()=>{return 3},
                Option.Some(item)=>{if item.n!=12 {return 4}}
            }
            match pod_hash_map.remove::<Key,Value>(&mut map,Key{n:7}) {
                .err(_)=>{return 5},
                .ok(item)=>{
                    let removed:Option<Value>=item
                    match removed {
                        Option.None()=>{return 6},
                        Option.Some(value)=>{if value.n!=12 {return 7}}
                    }
                }
            }
            return i32(pod_hash_map.len::<Key,Value>(&map))
        }
    }
}
