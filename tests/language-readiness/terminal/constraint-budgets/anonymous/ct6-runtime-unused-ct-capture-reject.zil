struct Phantom<life s,T:Copy,#N:i32>{n:i32}
fn scalar(marker:Type,n:i32)->i32{return n}
fn unused<T:Copy,#N:i32,life s>(marker:T,p:&'s i32)->i32{
    let value:Phantom<'s,T,N>=#(Phantom::<'s,T,N>{n:scalar(i32,marker)});
    return value.n
}
fn main()->i32{return 0}
