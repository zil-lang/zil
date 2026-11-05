struct Phantom<life a,T:Copy,#N:i32>{n:i32}
fn make<T:Copy,#N:i32,life a>(marker:Type)->Phantom<'a,T,N>{return Phantom::<'a,T,N>{n:N}}
fn relay<life b,#M:i32,U:Copy>(marker:Type)->Phantom<'b,U,M>{return make::<U,M,'b>(marker)}
fn result<life a,life b>(x:&'a i32,y:&'b i32)->i32{let left:Phantom<'a,i32,42>=#(relay::<'a,42,i32>(i32));let right:Phantom<'b,i32,7>=#(relay::<'b,7,i32>(i32));return left.n+right.n}
fn main()->i32{let n=0;return result(&n,&n)}
