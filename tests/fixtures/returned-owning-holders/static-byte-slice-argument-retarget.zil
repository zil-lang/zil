fn make()->impl FnMut(&[u8])->u8{
 var text="abc"
 return move(ignored:&[u8])=>{let size=ignored.len();let old=text[0];text="def";return old}
}
fn main()->i32{var f=make();return i32(f("x"))+i32(f("x"))-197}
