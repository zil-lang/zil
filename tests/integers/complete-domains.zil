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
    if 4294967295u32/3u32!=1431655765u32 || 4294967295u32%256u32!=255u32 || !(4294967295u32>2147483648u32) {return 11}
    if -127i8/3i8!=-42i8 || -127i8%3i8!=-1i8 || -32767i16/3i16!=-10922i16 || -32767i16%3i16!=-1i16 {return 12}
    if -9223372036854775807isize/3isize!=-3074457345618258602isize || -9223372036854775807isize%3isize!=-1isize {return 13}
    if 7i8*18i8!=126i8 || 127i16*257i16!=32639i16 || 65535u32*65537u32!=4294967295u32 || 42isize*42isize!=1764isize {return 14}
    if 64i8*%4i8!=0i8 || 256i16*%256i16!=0i16 || 65536u32*%65536u32!=0u32 || 4294967296isize*%4294967296isize!=0isize {return 15}
    if ~a!=127i8 || ~b!=32767i16 || ~c!=0u32 || ~d!=9223372036854775807isize {return 16}
    if (0xaaaaaaaau32&0x55555555u32)!=0u32 || (0xaaaaaaaau32|0x55555555u32)!=c || (c^0x80000000u32)!=2147483647u32 {return 17}
    let pure=#((65535u32*65537u32, -127i8/3i8, -32767i16%3i16, -9223372036854775807isize/3isize))
    if pure.0!=c || pure.1!=-42i8 || pure.2!=-1i16 || pure.3!=-3074457345618258602isize {return 18}
    return 0
}
