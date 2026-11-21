fn make()->impl FnMut()->u8{
 var text="abc"
 return move()=>{let old=text[0];text="def";return old}
}
fn main()->i32{var f=make();return i32(f())+i32(f())-197}
