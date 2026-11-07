fn identity<T:Copy>(value:T)->T{return value}
fn keep(value:u64)->u64{return value}
fn divide(value:u64)->u64{return value/3u64}
fn result()->i64{
    let highest=18446744073709551615u64;
    let call:fn(u64)->u64=keep;
    let n=identity(call(highest));
    if n/3u64!=6148914691236517205u64{return 1}
    if n%3u64!=0u64{return 2}
    if n<=9223372036854775808u64{return 3}
    if n>>63u64!=1u64{return 4}
    if n+%1u64!=0u64{return 5}
    if 1u64<<63u64!=9223372036854775808u64{return 6}
    if ~(0u64)!=n{return 7}
    let folded:u64=#(divide(18446744073709551615u64));
    if folded!=6148914691236517205u64{return 8}
    let pointer_size:usize=18446744073709551615usize;
    if pointer_size+%1usize!=0usize{return 9}
    if pointer_size>>63usize!=1usize{return 10}
    let converted=usize(n);
    if u64(converted)!=n{return 11}
    let small:u8=42;
    if u8(usize(small))!=small{return 12}
    return i64(u64(small))
}
