trait A{fn a<U:Copy>(self:&Self,value:U)->i32;}
trait B{fn b<U:Copy>(self:&Self,value:U)->i32;}
fn identity<T:Copy>(value:T)->T{return value}
trait Run:A+B{fn run<T:Copy>(self:&Self,value:T)->i32{let same=identity::<T>(value);return self.a(true)+self.b(true)}}
struct P{}
impl A for P{fn a<U:Copy>(value:U)->i32{return 7}}
impl B for P{fn b<U:Copy>(value:U)->i32{return 11}}
impl Run for P{}
fn main()->i32{let p=P{};return p.run::<([i32;1],[i32;1],[i32;1],[i32;1])>(([1],[2],[3],[4]))-18}
