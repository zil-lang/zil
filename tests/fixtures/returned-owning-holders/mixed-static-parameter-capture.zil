fn make<life a>(input:&'a [u8],constant:bool)->impl Fn()->u8{
 let text=if constant {"abc"}else{input}
 return move()=>text[0]
}
fn main()->i32{
 var bytes=[100u8,101u8]
 let f=make(&bytes[0..2],false)
 let g=make(&bytes[0..2],true)
 return i32(f())+i32(g())-197
}
