struct View<life s>{p:&'s i64}
struct Brand<T,life s>{n:i32}
fn value<life a,life b>(x:&'a i64,y:&'b i64)->i32{let brand:Brand<View<'a>,'a>=Brand::<View<'a>,'a>{n:42};return brand.n}
fn main()->i32{let x:i64=1;let y:i64=2;return value(&x,&y)}
