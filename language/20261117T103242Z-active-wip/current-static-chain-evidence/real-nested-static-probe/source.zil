fn make()->impl FnOnce()->&[u8]{
 let pair=("A",7i32)
 return move()=>{return pair.0}
}
fn main()->i32{
 let callback=make()
 let text=callback()
 return i32(text[0])-65
}
