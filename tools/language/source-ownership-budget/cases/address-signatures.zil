fn shared<F:Fn(&i32)->i32>(f:F)->impl Fn(&i32)->i32{
    return move(p:&i32)=>f(p)
}
fn mutating<F:Fn(&mut i32)->i32>(f:F)->impl Fn(&mut i32)->i32{
    return move(p:&mut i32)=>f(p)
}
fn slice<F:Fn(&[i32])->i32>(f:F)->impl Fn(&[i32])->i32{
    return move(p:&[i32])=>f(p)
}
fn main()->i32{
    let a=shared((p:&i32)=>*p)
    let n=11
    if a(&n)!=11{return 1}
    let b=mutating((p:&mut i32)=>{*p+=2;return *p})
    var changed=5
    if b(&mut changed)!=7{return 2}
    if b(&mut changed)!=9{return 3}
    let c=slice((p:&[i32])=>p[0]+i32(p.len()))
    let values=[8,11]
    return c(&values[..])-10
}
