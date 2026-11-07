trait A{fn a<U:Copy>(self:&Self,value:U)->i32;}
trait B{fn b<U:Copy>(self:&Self,value:U)->i32;}
trait Run{fn run(self:&Self)->i32;}
struct Data<T:Copy>{value:T}
impl<T:Copy> A for Data<T>{fn a<U:Copy>(value:U)->i32{return 7}}
impl<T:Copy> B for Data<T>{fn b<U:Copy>(value:U)->i32{return 11}}
struct Box<T:Copy>{value:T}
fn identity<T:Copy>(value:T)->T{return value}
impl<T:Copy+A+B> Run for Box<T>{fn run()->i32{let same=identity::<T>(.value);return .value.a(true)+.value.b(true)}}
fn main()->i32{let data=Data::<([i32;1],[i32;1],[i32;1],[i32;1])>{value:([1],[2],[3],[4])};let p=Box::<Data<([i32;1],[i32;1],[i32;1],[i32;1])>>{value:data};return p.run()-18}
