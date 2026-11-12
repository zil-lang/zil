
struct Holder<life a>{pointer:&'a i32}
fn hold<life a>(pointer:&'a i32)->Holder<'a>{return Holder::<'a>{pointer:pointer}}
fn make<life a>(holder:&Holder<'a>)->impl Fn()->i32{let pointer=holder.pointer;return move()=>*pointer}
fn main()->i32{var n:i32=41;let other:i32=9;var holder=hold(&n);let f=make(&holder);holder.pointer=&other;return f()-41}