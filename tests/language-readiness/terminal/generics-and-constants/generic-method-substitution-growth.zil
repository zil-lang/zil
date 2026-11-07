trait A{fn a<U:Copy>(self:&Self,value:U)->i32;}
trait B{fn b<U:Copy>(self:&Self,value:U)->i32;}
struct P{}
impl A for P{fn a<U:Copy>(value:U)->i32{return 7}}
impl B for P{fn b<U:Copy>(value:U)->i32{return 11}}
fn identity<T:Copy>(value:T)->T{return value}
fn choose<T:Copy,V:A+B>(p:&V,value:T)->i32{let same=identity::<T>(value);return p.a(true)+p.b(true)}
fn main()->i32{let p=P{};return choose::<([i32;1],[i32;1],[i32;1],[i32;1]),P>(&p,([1],[2],[3],[4]))-18}
