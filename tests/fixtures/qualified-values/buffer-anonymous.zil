fn identity<T:Value>(value:T)->T{return value}
fn read<life a>(input:&'a i32)->i32{
 let buffer=bytes_new()
 if !bytes_reserve(buffer,2){return 1}
 if !bytes_push(buffer,65){return 2}
 let pair=identity::<(Managed<ByteBuffer>,&'a i32)>((give buffer,input))
 let (bytes,view)=pair
 if bytes_len(bytes)!=1{return 3}
 let data=bytes_view(bytes)
 return *view+i32(data[0])-65
}
fn main()->i32{var input:i32=42;return read(&input)-42}
