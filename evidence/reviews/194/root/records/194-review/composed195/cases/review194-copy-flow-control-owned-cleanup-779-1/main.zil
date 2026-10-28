
extern "C" fn putchar(n:i32)->i32;
resource Ticket{n:i32}impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.n)}}}
fn fold<T:Copy,F:FnMut(T)->T>(f:&mut F,initial:T,n:i32)->T{
    var value=initial;
    for index in 0..n{
        if index==1{continue}
        if index>=5{break}
        value=f(value)
    }
    return value
}
fn main()->i32{
    let ticket=Ticket{n:65};var calls=0;
    var f=move(x:[i64;2])=>{ticket.n;calls+=1;return [x[0]+x[1],x[1]+1]};
    let initial:[i64;2]=[0,1];
    let zero=fold(&mut f,initial,0);
    let result=fold(&mut f,initial,6);
    return i32(zero[0]+zero[1]+result[0]+result[1])-16
}
