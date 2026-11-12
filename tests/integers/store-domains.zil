struct Mixed {small:i8; wide:isize; unsigned:u32; pair:[i16;2]}
fn exchange<T:Copy+StoreElement>(value:T)->void!T {
    let items:Store<T>=store_new()
    if !store_reserve(items,1){return ()}
    match store_push(items,value){.err(_)=>{return ()},.ok(_)=>{}}
    if store_len(items)!=1{return ()}
    return store_pop(items)
}
fn verify()->void!i32 {
    if exchange(255u8)?!=255u8{return 1}
    if exchange(65535u16)?!=65535u16{return 2}
    if exchange(4294967295u32)?!=4294967295u32{return 3}
    if exchange(18446744073709551615u64)?!=18446744073709551615u64{return 4}
    if exchange(18446744073709551615usize)?!=18446744073709551615usize{return 5}
    if exchange(-128i8)?!=-128i8{return 6}
    if exchange(-32768i16)?!=-32768i16{return 7}
    if exchange(-2147483648i32)?!=-2147483648i32{return 8}
    if exchange(-9223372036854775808i64)?!=-9223372036854775808i64{return 9}
    if exchange(-9223372036854775808isize)?!=-9223372036854775808isize{return 10}
    let row=exchange(Mixed{small:-128i8,wide:-9223372036854775808isize,unsigned:4294967295u32,pair:[-32768i16,32767i16]})?
    if row.small!=-128i8 || row.wide!=-9223372036854775808isize || row.unsigned!=4294967295u32 || row.pair[0]!=-32768i16 || row.pair[1]!=32767i16{return 11}
    let tuple=exchange((4294967295u32,-128i8,[-32768i16,32767i16],-9223372036854775808isize))?
    if tuple.0!=4294967295u32 || tuple.1!=-128i8 || tuple.2[1]!=32767i16 || tuple.3!=-9223372036854775808isize{return 12}
    return 0
}
fn main()->i32 {match verify(){.err(_)=>{return 99},.ok(code)=>{return code}}}
