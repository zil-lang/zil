struct Phantom<life a,T:Copy,#N:i32>{n:i32}
fn make<T:Copy,#N:i32,life a>(marker:Type)->Phantom<'a,T,N>{return Phantom::<'a,T,N>{n:N}}
fn middle<life b,#M:i32,U:Copy>(marker:Type)->Phantom<'b,U,M>{return make::<U,M,'b>(marker)}
fn forward<#N:i32,life a,T:Copy>(marker:Type)->Phantom<'a,T,N>{return middle::<'a,N,T>(marker)}
fn result<life s>(p:&'s i32)->i32{let value:Phantom<'s,i32,42>=#(forward::<42,'s,i32>(i32));return value.n}
fn main()->i32{let n=0;return result(&n)}
