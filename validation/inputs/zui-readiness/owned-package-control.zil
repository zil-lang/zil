package View{p:i64;manages bytes:ByteBuffer}fn make(p:i64)->View{let b=bytes_new();return View{p:p,bytes:give b}}fn main()->i32{let v=make(42);return i32(v.p)}
