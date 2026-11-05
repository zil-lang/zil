struct Phantom<life a,T:Copy,#N:i32>{n:i32}
fn make<T:Copy,#N:i32,life a>(marker:Type,n:i32)->Phantom<'a,T,N>{
    return Phantom::<'a,T,N>{n:n}
}
fn result<life s>(p:&'s i32)->i32{
    let value:Phantom<'s,i32,7>=#(make::<i32,7,'s>(i32,42));
    return value.n
}
fn main()->i32{let n=0;return result(&n)}
