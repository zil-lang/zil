struct Mixed{word:u64,amount:f64}
enum Carrier{Value(Mixed,usize),Empty}
fn to_float(value:u64)->f64{return f64(value)}
fn artifact(t:Type)->u64{return u64(18446744073709549568.0)}
fn result()->i32{
    let signed_edge:u64=9223372036854775808u64;
    if f64(signed_edge)!=9223372036854775808.0{return 1}
    if u64(f64(signed_edge))!=signed_edge{return 2}
    let last: f64=18446744073709549568.0;
    let integer:u64=u64(last);
    if integer!=18446744073709549568u64{return 3}
    if f64(integer)!=last{return 4}
    if f64(18446744073709551615u64)!=18446744073709551616.0{return 5}
    if f32(18446744073709551615u64)!=18446744073709551616.0f32{return 6}
    let pointer:usize=usize(last);
    if u64(pointer)!=integer{return 7}
    if f64(pointer)!=last{return 8}
    if u64(-0.0)!=0u64{return 9}
    let folded:u64=#(u64(18446744073709549568.0));
    if folded!=integer{return 10}
    let call:fn(u64)->f64=to_float;
    if call(integer)!=last{return 11}
    let generated:u64=#(artifact(i32));
    if generated!=integer{return 12}
    let high:f64=#(f64(18446744073709551615u64));
    if high!=18446744073709551616.0{return 13}
    let carrier=Carrier.Value(Mixed{word:integer,amount:last},pointer);
    match carrier{
        Carrier.Value(pair,size)=>{if pair.word==integer && pair.amount==last && u64(size)==integer{return 344}return 14},
        Carrier.Empty()=>{return 15}
    }
}
