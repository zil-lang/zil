resource Mark{code:i32}
fn make(label:i32)->impl Fn(i32)->i32{
    let mark=Mark{code:label}
    return move(value:i32)=>mark.code+value
}
fn wrong<F:Fn(i32)->i32>(input:F,right:bool)->F{
    if right{return input}
    return make(66)
}
fn main()->i32{return wrong(make(65),false)(0)-65}
