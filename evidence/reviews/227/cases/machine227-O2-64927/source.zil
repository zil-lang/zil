
extern "C" fn putchar(code:i32)->i32;
fn emit(code:i32){unsafe{putchar(code)}}
fn main()->void!i32{
 let first=bytes_new()
 if !bytes_reserve(first,7){return ()}
 defer{let consumed=give first;emit(65)}
 let second=bytes_new()
 if !bytes_reserve(second,11){return ()}
 defer{let inner=bytes_new();let prepared=bytes_reserve(inner,13);emit(66)}
 return 0
}