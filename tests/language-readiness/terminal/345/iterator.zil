import std.iter
struct Packet{value:u64;tag:i32}
fn main()->i32 {
    let values=[10,20,12]
    var sum=0
    var callback=(value:i32)=>{sum+=value}
    iter.for_each(&values[..],&mut callback)
    if sum!=42{return 1}
    var add=(a:i32,b:i32)=>a+b
    if iter.fold(&values[..],0,&mut add)!=42{return 2}
    var any_calls=0
    var twenty_any=(value:i32)=>{any_calls+=1;return value==20}
    if !iter.any(&values[..],&mut twenty_any){return 3}
    if any_calls!=2{return 4}
    var find_calls=0
    var twenty_find=(value:i32)=>{find_calls+=1;return value==20}
    let found=iter.find(&values[..],&mut twenty_find)
    match found {Option.Some(value)=>{if value!=20{return 5}},Option.None()=>{return 6}}
    if find_calls!=2{return 7}
    var twenty=(value:i32)=>value==20
    let position=iter.position(&values[..],&mut twenty)
    match position {Option.Some(index)=>{if index!=1{return 8}},Option.None()=>{return 9}}
    var positive=(value:i32)=>value>0
    if !iter.all(&values[..],&mut positive){return 10}
    let empty:[i32;0]=[]
    var empty_calls=0
    var empty_callback=(value:i32)=>{empty_calls+=1;return value==20}
    if iter.any(&empty[..],&mut empty_callback){return 11}
    if !iter.all(&empty[..],&mut empty_callback){return 12}
    if iter.fold(&empty[..],42,&mut add)!=42{return 13}
    match iter.find(&empty[..],&mut empty_callback){Option.Some(_)=>{return 14},Option.None()=>{}}
    if empty_calls!=0{return 15}
    let packets=[Packet{value:0u64,tag:1},Packet{value:18446744073709551615u64,tag:2}]
    var wide=(packet:Packet)=>packet.value>9223372036854775808u64
    match iter.find(&packets[..],&mut wide){Option.Some(packet)=>{if packet.tag!=2{return 16}},Option.None()=>{return 17}}
    return 0
}
