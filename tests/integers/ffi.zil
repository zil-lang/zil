struct Numbers { a:i8; b:i16; c:u32; d:isize }
extern "C" fn echo_signed_byte(value:i8)->i8;
extern "C" fn echo_signed_word(value:i16)->i16;
extern "C" fn echo_unsigned_word(value:u32)->u32;
extern "C" fn echo_native_word(value:isize)->isize;
extern "C" fn inspect_numbers(value:&Numbers)->i32 contract{provides(noescape(value));effects(ffi);};
extern "C" fn mutate_numbers(value:&mut Numbers) contract{provides(noescape(value));effects(ffi);};
extern "C" fn invoke_integer_callbacks(a:extern "C" fn(i8)->i8,again:extern "C" fn(i8)->i8,b:extern "C" fn(i16)->i16,c:extern "C" fn(u32)->u32,d:extern "C" fn(isize)->isize)->i32 contract{provides(static_callbacks());effects(ffi);};
fn callback8(value:i8)->i8{return value}
fn callback16(value:i16)->i16{return value}
fn callback32(value:u32)->u32{return value}
fn callbacknative(value:isize)->isize{return value}
fn result()->i32 {
    var n=Numbers{a:-128i8,b:-32768i16,c:4294967295u32,d:-9223372036854775808isize}
    unsafe {
        if echo_signed_byte(n.a)!=n.a || echo_signed_word(n.b)!=n.b || echo_unsigned_word(n.c)!=n.c || echo_native_word(n.d)!=n.d {return 1}
        if inspect_numbers(&n)!=0 {return 2}
        mutate_numbers(&mut n)
        if invoke_integer_callbacks(c_callback(callback8),c_callback(callback8),c_callback(callback16),c_callback(callback32),c_callback(callbacknative))!=42 {return 4}
    }
    if n.a!=127i8 || n.b!=32767i16 || n.c!=2147483648u32 || n.d!=9223372036854775807isize {return 3}
    return 0
}
