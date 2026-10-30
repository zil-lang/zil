fn lender(n:i32)->impl Fn()->&i32{return move()=>&n}
fn reader(n:i32)->impl Fn()->i32{
    let child=lender(n)
    return move()=>{let value=child();return *value}
}
fn wrap<F:Fn()->i32>(f:F)->impl Fn()->i32{return move()=>f()}
fn main()->i32{let f=wrap(reader(21));return f()+f()-42}
