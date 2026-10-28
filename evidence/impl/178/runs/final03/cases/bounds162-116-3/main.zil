
trait Measure {fn len(self:&Self)->i32;fn plus(self:&Self,other:&Self)->i32;}
struct Number {n:i32}
impl Measure for Number {fn len()->i32=>.n;fn plus(other:&Self)->i32=>.n+other.n}
fn combine<T:Measure>(a:&T,b:&T)->i32=>a.len()+a.plus(b)
fn main()->i32{let a=Number{n:7};let b=Number{n:28};return combine(&a,&b)-42}
