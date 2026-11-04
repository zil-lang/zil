import iter
fn main()->i32{
    let values=[10,20,12]
    var sum=0
    var callback=(value:i32)=>{sum+=value}
    iter.for_each(&values[..],&mut callback)
    if sum!=42{return 1}
    var add=(a:i32,b:i32)=>a+b
    if iter.fold(&values[..],0,&mut add)!=42{return 2}
    return 0
}
