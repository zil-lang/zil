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
    if remaining_scalars()!=0{return 5}
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

struct WideNumbers {a:u64;b:usize;c:f32;d:f64}
extern "C" fn echo_u64(value:u64)->u64;
extern "C" fn echo_usize(value:usize)->usize;
extern "C" fn inspect_wide(value:&WideNumbers)->i32 contract{provides(noescape(value));effects(ffi);};
extern "C" fn mutate_wide(value:&mut WideNumbers) contract{provides(noescape(value));effects(ffi);};
extern "C" fn invoke_remaining_callbacks(a:extern "C" fn(u8)->u8,again:extern "C" fn(u8)->u8,b:extern "C" fn(u16)->u16,c:extern "C" fn(u64)->u64,d:extern "C" fn(usize)->usize,e:extern "C" fn(i32)->i32,f:extern "C" fn(i64)->i64,g:extern "C" fn(f32)->f32,h:extern "C" fn(f64)->f64)->i32 contract{provides(static_callbacks());effects(ffi);};
fn callback_u8(value:u8)->u8{return value}
fn callback_u16(value:u16)->u16{return value}
fn callback_u64(value:u64)->u64{return value}
fn callback_usize(value:usize)->usize{return value}
fn callback_i32(value:i32)->i32{return value}
fn callback_i64(value:i64)->i64{return value}
fn callback_f32(value:f32)->f32{return value}
fn callback_f64(value:f64)->f64{return value}
fn remaining_scalars()->i32 {
    var n=WideNumbers{a:18446744073709551615u64,b:18446744073709551615usize,c:-1.25f32,d:3.5f64}
    unsafe {
        if echo_u64(n.a)!=n.a || echo_usize(n.b)!=n.b || inspect_wide(&n)!=0{return 1}
        if invoke_remaining_callbacks(c_callback(callback_u8),c_callback(callback_u8),c_callback(callback_u16),c_callback(callback_u64),c_callback(callback_usize),c_callback(callback_i32),c_callback(callback_i64),c_callback(callback_f32),c_callback(callback_f64))!=42{return 2}
        mutate_wide(&mut n)
    }
    if n.a!=9223372036854775808u64 || n.b!=42usize || n.c!=2.5f32 || n.d!=-7.0f64{return 3}
    return 0
}
