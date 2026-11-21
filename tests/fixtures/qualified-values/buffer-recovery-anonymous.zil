fn identity<T:Value>(value:T)->T{return value}
fn read<life a>(input:&'a i32)->i32{
 let buffer=bytes_new()
 if !bytes_reserve(buffer,1){return 1}
 if !bytes_push(buffer,65){return 2}
 let pair=identity::<(Managed<ByteBuffer>,&'a i32)>((give buffer,input))
 let (bytes,view)=pair
 if !bytes_reserve(bytes,64){
  if bytes_len(bytes)!=1{return 3}
  let old=bytes_view(bytes)
  if old[0]!=65{return 4}
  if !bytes_reserve(bytes,64){return 5}
 }
 if bytes_len(bytes)!=1{return 6}
 let data=bytes_view(bytes)
 return *view+i32(data[0])-65
}
fn main()->i32{var input:i32=42;return read(&input)-42}
