struct Phantom<life a,T:Copy,#N:i32>{n:i32}
fn make<T:Copy,#N:i32,life a,life spare>(marker:Type)->Phantom<'a,T,N>{return Phantom::<'a,T,N>{n:N}}
fn middle<life b,life spare,#M:i32,U:Copy>(marker:Type)->Phantom<'b,U,M>{return make::<U,M,'b,'spare>(marker)}
fn forward<#N:i32,life a,life spare,T:Copy>(marker:Type)->Phantom<'a,T,N>{return middle::<'a,'spare,N,T>(marker)}
fn result<life s,life t>(p:&'s i32,q:&'t i32)->i32{let value:Phantom<'s,i32,42>=#(forward::<42,'s,'t,i32>(i32));return value.n}
fn main()->i32{let n=0;return result(&n,&n)}
