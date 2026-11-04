import std.iter
fn main()->i32 {
    let values=[10,20,12]
    var sum=0
    var callback=(value:i32)=>{sum+=value}
    iter.for_each(&values[..],&mut callback)
    if sum!=42{return 1}
    var add=(a:i32,b:i32)=>a+b
    if iter.fold(&values[..],0,&mut add)!=42{return 2}
    var calls=0
    var twenty=(value:i32)=>{calls+=1;return value==20}
    if !iter.any(&values[..],&mut twenty) || calls!=2{return 3}
    calls=0
    let found=iter.find(&values[..],&mut twenty)
    match found {Option.Some(value)=>{if value!=20 || calls!=2{return 4}},Option.None()=>{return 5}}
    let position=iter.position(&values[..],&mut twenty)
    match position {Option.Some(index)=>{if index!=1{return 6}},Option.None()=>{return 7}}
    var positive=(value:i32)=>value>0
    if !iter.all(&values[..],&mut positive){return 8}
    let empty:[i32;0]=[]
    calls=0
    if iter.any(&empty[..],&mut twenty) || calls!=0{return 9}
    if !iter.all(&empty[..],&mut twenty) || calls!=0{return 10}
    if iter.fold(&empty[..],42,&mut add)!=42{return 11}
    match iter.find(&empty[..],&mut twenty){Option.Some(_)=>{return 12},Option.None()=>{}}
    return 0
}
