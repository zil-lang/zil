extern "C" fn putchar(value:i32)->i32;
struct Inner{unit:();flag:bool;tiny:u8;short:u16;negative:i32}
struct Sample{inner:Inner;pair:(u64,usize);lanes:[i64;2];single:f32;double:f64;tag:i32}
fn sample(tag:i32)->Sample{return Sample{inner:Inner{unit:(),flag:true,tiny:251u8,short:65001u16,negative:-1234567},pair:(18446744073709551615u64,4294967295usize),lanes:[-9223372036854775807,9223372036854775807],single:1.25f32,double:-9.5f64,tag:tag}}
fn intact(value:Sample,tag:i32)->bool{return value.inner.flag && value.inner.tiny==251u8 && value.inner.short==65001u16 && value.inner.negative== -1234567 && value.pair.0==18446744073709551615u64 && value.pair.1==4294967295usize && value.lanes[0]== -9223372036854775807 && value.lanes[1]==9223372036854775807 && value.single==1.25f32 && value.double== -9.5f64 && value.tag==tag}
fn copy<T:Copy>(value:T)->T{return value}
fn relay(value:Sample!void)->Sample!void{return value}
fn pop(items:Store<Sample>)->void!Sample{return store_pop(items)}
fn main()->i32{
    let items:Store<Sample>=store_new()
    if store_len(items)!=0 || store_capacity(items)!=0{return 1}
    match store_pop(items){.ok(_)=>{return 2},.err(())=>{}}
    if !store_reserve(items,2){return 3}
    var first=sample(41)
    let inserted=store_push(items,first)
    let inserted_copy=copy(inserted)
    match inserted{.ok(())=>{},.err(_)=>{return 4}}
    match inserted_copy{.ok(())=>{},.err(_)=>{return 5}}
    first.inner.tiny=1u8
    first.tag=99
    store_push(items,sample(42))
    var rejected=sample(43)
    let failed=relay(store_push(items,rejected))
    rejected.inner.short=1u16
    rejected.tag=100
    let failed_copy=copy(failed)
    match failed{.ok(())=>{return 6},.err(value)=>{if !intact(value,43){return 7}}}
    match failed_copy{.ok(())=>{return 8},.err(value)=>{if !intact(value,43){return 9}}}
    if store_len(items)!=2 || store_capacity(items)!=2{return 10}
    if store_reserve(items,-1) || store_reserve(items,9223372036854775807){return 11}
    if store_len(items)!=2 || store_capacity(items)!=2{return 12}
    if !intact(*store_at(items,0),41) || !intact(*store_at(items,1),42){return 13}
    let popped=pop(items)
    let popped_copy=copy(popped)
    match popped{.ok(value)=>{if !intact(value,42){return 14}},.err(())=>{return 15}}
    match popped_copy{.ok(value)=>{if !intact(value,42){return 16}},.err(())=>{return 17}}
    match store_pop(items){.ok(value)=>{if !intact(value,41){return 18}},.err(())=>{return 19}}
    match store_pop(items){.ok(_)=>{return 20},.err(())=>{}}
    if store_len(items)!=0 || store_capacity(items)!=2{return 21}
    unsafe{putchar(80);putchar(79);putchar(68);putchar(10)}
    return 0
}
