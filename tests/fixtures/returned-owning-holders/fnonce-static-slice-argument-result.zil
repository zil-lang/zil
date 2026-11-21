resource Token{value:i32}
fn consume(give token:Token){}
fn make(give token:Token)->impl FnOnce(&[u8])->&[u8]{
 let text="abc"
 return move(ignored:&[u8])=>{let size=ignored.len();consume(give token);return text}
}
fn main()->i32{
 let f=make(Token{value:1})
 let text=f("x")
 return i32(text[0])+i32(text.len())-100
}
