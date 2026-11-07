import std.i64_hash_map
fn put(map:&mut i64_hash_map.I64HashMap,key:i64,value:i64,previous:i64)->i32 {
    match i64_hash_map.insert(map,key,value) {
        .err(_)=>{return 1},
        .ok(old)=>{
            let prior:Option<i64>=old
            match prior {
                Option.None()=>{if previous!=-1 {return 2}},
                Option.Some(found)=>{if found!=previous {return 3}}
            }
        }
    }
    return 0
}
fn read(map:&i64_hash_map.I64HashMap,key:i64)->i64 {
    match i64_hash_map.get(map,key) {
        Option.None()=>{return -1},
        Option.Some(value)=>{return value}
    }
}
fn erase(map:&mut i64_hash_map.I64HashMap,key:i64,expected:i64)->i32 {
    match i64_hash_map.remove(map,key) {
        .err(_)=>{return 1},
        .ok(old)=>{
            let prior:Option<i64>=old
            match prior {
                Option.None()=>{if expected!=-1 {return 2}},
                Option.Some(value)=>{if value!=expected {return 3}}
            }
        }
    }
    return 0
}
fn main()->i32 {
    match i64_hash_map.with_capacity(4) {
        .err(_)=>{return 1},
        .ok(value)=>{
            var map:i64_hash_map.I64HashMap=value
            if put(&mut map,1,11,-1)!=0 {return 2}
            if put(&mut map,10,20,-1)!=0 {return 3}
            if put(&mut map,19,30,-1)!=0 {return 4}
            if put(&mut map,-8,40,-1)!=0 {return 5}
            match i64_hash_map.at(&map,0) {Option.None()=>{return 15},Option.Some(pair)=>{if pair.key!=1 || pair.value!=11{return 16}}}
            if put(&mut map,10,22,20)!=0 {return 6}
            if read(&map,1)!=11 || read(&map,10)!=22 || read(&map,19)!=30 || read(&map,-8)!=40 {return 7}
            if erase(&mut map,1,11)!=0 {return 8}
            match i64_hash_map.at(&map,0) {Option.None()=>{return 17},Option.Some(pair)=>{if pair.key!=-8 || pair.value!=40{return 18}}}
            if read(&map,1)!=-1 || read(&map,-8)!=40 || read(&map,10)!=22 || read(&map,19)!=30 {return 9}
            if put(&mut map,28,50,-1)!=0 {return 10}
            if i64_hash_map.len(&map)!=4 || read(&map,28)!=50 {return 11}
            match i64_hash_map.insert(&mut map,i64(37),i64(60)) {
                .ok(_)=>{return 12},
                .err(_)=>{}
            }
            if i64_hash_map.len(&map)!=4 || read(&map,37)!=-1 {return 13}
            if erase(&mut map,19,30)!=0 || read(&map,10)!=22 || read(&map,28)!=50 {return 14}
            return 0
        }
    }
}
