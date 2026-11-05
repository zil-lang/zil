struct First<life a,T:Copy,#N:i32>{n:i32}
struct Second<life a,T:Copy,#N:i32>{n:i32}
fn make<T:Copy,#N:i32,life a>(marker:Type)->Second<'a,T,N>{return Second::<'a,T,N>{n:N}}
fn relay<T:Copy,#N:i32,life a>(marker:Type)->First<'a,T,N>{return make::<T,N,'a>(marker)}
fn result<life a>(x:&'a i32)->i32{let value:First<'a,i32,42>=#(relay::<i32,42,'a>(i32));return value.n}
fn main()->i32{let n=0;return result(&n)}
