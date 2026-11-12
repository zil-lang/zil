fn make<life a>(bytes:&'a mut [u8])->impl FnOnce()->&'a mut [u8]{return move()=>give bytes}
fn main()->i32{
 var bytes=[20u8,21u8]
 let f=make(&mut bytes[..])
 let view=f()
 view[1]+=1u8
 let result=i32(view[0])+i32(view[1])
 bytes[0]=21u8
 return result+i32(bytes[0])-63
}
