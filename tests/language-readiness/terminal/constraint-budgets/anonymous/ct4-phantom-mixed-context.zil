struct Phantom<life s,T:Copy,#N:i32>{n:i32}
fn scalar(marker:Type,n:i32)->i32{return n}
fn result<life s>(p:&'s i32)->i32{
    let value:Phantom<'s,i32,7>=#(Phantom::<'s,i32,7>{n:scalar(i32,42)});
    return value.n
}
fn main()->i32{let n=0;return result(&n)}
