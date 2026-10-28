fn read(b:ByteBuffer)->u64=>bytes_len(b);fn main()->i32{let b=bytes_new();let f=()=>read(b);return i32(f())}
