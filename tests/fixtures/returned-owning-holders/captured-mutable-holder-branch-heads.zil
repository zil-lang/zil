struct Holder<life a>{pointer:&'a i32}
fn hold<life a>(pointer:&'a i32)->Holder<'a>{return Holder::<'a>{pointer:pointer}}
fn make<life a>(holder:&mut Holder<'a>,other:&'a i32)->impl FnMut()->i32{
 return move()=>{let old=holder.pointer;holder.pointer=other;return *old}
}
fn main()->i32{let choose=true;var a=20;var b=22;var left=hold(&a);var right=hold(&a);var f=make(if choose {&mut left}else{&mut right},&b);f();let x=*left.pointer;let y=*right.pointer;return x+y-42}
