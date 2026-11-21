fn main()->i32{
 let outer:Store<Managed<ByteBuffer>>=store_new()
 if !store_reserve(outer,1){return 1}
 let bytes=bytes_new()
 if !bytes_reserve(bytes,2){return 2}
 bytes_push(bytes,65)
 bytes_push(bytes,66)
 store_push(outer,give bytes)
 if !store_reserve(outer,2){return 3}
 match store_pop(outer){.ok(moved)=>{let view=bytes_view(moved);return i32(view[0])+i32(view[1])-131},.err(_)=>{return 4}}
}
