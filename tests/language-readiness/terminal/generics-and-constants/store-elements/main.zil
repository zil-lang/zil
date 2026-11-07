import api
struct Key{n:i32}
struct Value{n:i32}
fn main()->i32{
    let pairs:Store<(Key,Value)>=store_new()
    let copies:Store<[Key;2]>=store_new()
    match store_push(pairs,(Key{n:17},Value{n:18})) {
        .ok(_)=>{return 6},
        .err(rejected)=>{if rejected.0.n!=17 || rejected.1.n!=18{return 7}}
    }
    match store_push(copies,[Key{n:19},Key{n:20}]) {
        .ok(_)=>{return 8},
        .err(rejected)=>{if rejected[0].n!=19 || rejected[1].n!=20{return 9}}
    }
    if store_reserve(pairs,-1) || store_reserve(copies,-1){return 10}
    if store_len(pairs)!=0 || store_len(copies)!=0{return 11}
    if !store_reserve(pairs,1){return 1}
    if !store_reserve(copies,1){return 2}
    if !api.push_pair::<Key,Value>(pairs,Key{n:7},Value{n:8}){return 3}
    if !api.push_copies::<Key>(copies,Key{n:9}){return 4}
    var basket=api.build::<Key,Value>(give pairs,give copies)
    let before=api.first::<Key,Value>(&basket).n
    api.replace::<Key,Value>(&mut basket,Key{n:11})
    let copy=api.copy_at::<Key,Value>(&basket).n
    match api.pop_pair::<Key,Value>(&mut basket){Option.None()=>{return 5},Option.Some(pair)=>{return before+copy+pair.1.n-26}}
}
