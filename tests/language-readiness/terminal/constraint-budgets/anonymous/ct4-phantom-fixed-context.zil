struct Phantom<life s>{n:i32}
fn scalar(marker:Type,n:i32)->i32{return n}
fn result<life s>(p:&'s i32)->i32{
    let value:Phantom<'s>=#(Phantom::<'s>{n:scalar(i32,42)});
    return value.n
}
fn main()->i32{let n=0;return result(&n)}
