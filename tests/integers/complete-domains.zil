struct Numbers { a:i8; b:i16; c:u32; d:isize }
enum Choice { Small(i8), Wide(u32), Native(isize) }
fn identity<T:Copy>(x:T)->T {return x}
fn inc8(x:i8)->i8 {return x+1}
fn inc16(x:i16)->i16 {return x+1}
fn inc32(x:u32)->u32 {return x+1}
fn incnative(x:isize)->isize {return x+1}
fn result()->i32 {
    let a:i8 = -128
    let b:i16 = -32768
    let c:u32 = 4294967295
    let d:isize = -9223372036854775808
    if identity(a)!=-128i8 || identity(b)!=-32768i16 || identity(c)!=4294967295u32 || identity(d)!=-9223372036854775808isize {return 1}
    let n=Numbers{a:a,b:b,c:c,d:d}
    if n.a!=a || n.b!=b || n.c!=c || n.d!=d {return 2}
    let values:[i16;3]=[-32768i16,42i16,32767i16]
    let index:u32=1
    if values[index]!=42i16 {return 3}
    let value=Choice.Wide(c)
    var selected:u32=0
    match value {Choice.Small(x)=>{selected=u32(x)},Choice.Wide(x)=>{selected=x},Choice.Native(x)=>{selected=u32(x)}}
    if selected!=c {return 4}
    if (127i8 +% 1i8)!=-128i8 || (32767i16 +% 1i16)!=-32768i16 || (4294967295u32 +% 1u32)!=0u32 || (9223372036854775807isize +% 1isize)!=d {return 5}
    if (a>>7i8)!=-1i8 || (b>>15i16)!=-1i16 || (c>>31u32)!=1u32 || (d>>63isize)!=-1isize {return 6}
    if (1i8<<7i8)!=a || (1i16<<15i16)!=b || (1u32<<31u32)!=2147483648u32 || (1isize<<63isize)!=d {return 7}
    if inc8(41i8)!=42i8 || inc16(41i16)!=42i16 || inc32(41u32)!=42u32 || incnative(41isize)!=42isize {return 8}
    let ct=#((inc8(41i8), inc16(41i16), inc32(41u32), incnative(41isize)))
    if ct.0!=42i8 || ct.1!=42i16 || ct.2!=42u32 || ct.3!=42isize {return 9}
    if i8(f64(42u32))!=42i8 || i16(f32(42isize))!=42i16 || u32(4294967295.0)!=c || isize(-9223372036854775808.0)!=d {return 10}
    return 0
}
