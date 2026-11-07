trait Left<Tag>{fn left<T:Copy>(self:&Self,value:T)->i32;}
trait Right<Tag>{fn right<T:Copy>(self:&Self,value:T)->i32;}
struct P{}
impl Left<bool> for P{fn left<T:Copy>(value:T)->i32{return 7}}
impl Right<bool> for P{fn right<T:Copy>(value:T)->i32{return 11}}
fn choose<V:Left<bool>+Right<bool>>(p:&V)->i32{return p.left(true)+p.right(true)}
fn main()->i32{let p=P{};return choose(&p)-18}
