struct First{n:i32}
struct Second{n:i32}
fn choose<T:Copy,#N:i32,life a>(marker:Type)->Type{return T}
fn relay<life b,#M:i32,U:Copy>(marker:Type)->Type{return choose::<U,M,'b>(marker)}
fn result<life a>(x:&'a i32)->i32{let value:#(relay::<'a,7,First>(i32))=Second{n:42};return value.n}
fn main()->i32{let n=0;return result(&n)}
