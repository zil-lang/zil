trait A{fn a<U:Copy>(self:&Self,value:U)->i32;}
trait B{fn b<U:Copy>(self:&Self,value:U)->i32;}
struct P{}
impl A for P{fn a<U:Copy>(value:U)->i32{return 7}}
impl B for P{fn b<U:Copy>(value:U)->i32{return 11}}
fn constant<#N:i32>()->i32{return N}
fn choose<#N:i32,V:A+B>(p:&V)->i32{let a=constant::<N>();let b=constant::<N>();let c=constant::<N>();let d=constant::<N>();return p.a(true)+p.b(true)}
fn main()->i32{let p=P{};return choose::<7,P>(&p)-18}
