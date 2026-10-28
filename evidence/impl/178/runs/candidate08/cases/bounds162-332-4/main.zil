
trait Read { fn get(self:&Self)->i32; }
trait Change { fn set(self:&mut Self,n:i32); }
trait Consume { fn finish(self:Self)->i32; }
struct Counter { n:i32 }
impl Read for Counter { fn get()->i32=>.n }
impl Change for Counter { fn set(n:i32){.n=n} }
impl Consume for Counter { fn finish()->i32=>.n }
fn update<T:Read+Change>(value:&mut T,n:i32)->i32{value.set(n);return value.get()}
fn local<T:Read+Change>(value:T,n:i32)->i32{var x=value;x.set(n);return x.get()}
fn finish<T:Consume>(value:T)->i32=>value.finish()
fn main()->i32 {var c=Counter{n:1};if update(&mut c,20)!=20{return 1};if local(c,40)!=40{return 2};return finish(Counter{n:42})-42}
