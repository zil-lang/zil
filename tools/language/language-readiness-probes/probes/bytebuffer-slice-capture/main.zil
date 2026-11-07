fn main()->i32{let b=bytes_new();if !bytes_reserve(b,1){return 1};bytes_push(b,u8(7));let view=bytes_view(b);let f=()=>view[0];return i32(f())-7}
