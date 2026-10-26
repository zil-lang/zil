import model
import factory
import io

resource Token { code: i32 }
impl Drop for Token {
    fn drop(self: Self) { io.emit(.code); }
}
package Buffers { manages b: ByteBuffer; manages c: ByteBuffer }

fn value() -> i64 {
    let token = Token { code: 84 };
    var p = Buffers { b: bytes_new(), c: bytes_new() };
    if !bytes_reserve(p.b, 4) { return -1; }
    if !bytes_push(p.b, 2u8) { return -1; }
    let view = bytes_view(p.b);
    let old = give p.c;
    p.c = bytes_new();
    if !bytes_reserve(p.c, 16) { return -2; }
    if !bytes_push(p.c, 40u8) { return -3; }
    let seed = #(model.pack(10));
    var first = factory.make(model.unpack(seed));
    var second = factory.make(20);
    let a = first(model.pack(2));
    let b = second(model.pack(3));
    return a + b + first(model.pack(1)) + i64(view[0]) + bytes_len(old);
}
