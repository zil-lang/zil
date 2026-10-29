extern "C" fn putchar(n:i32)->i32;fn emit(n:i32){unsafe{putchar(n)}}
fn lower(value:i64)->i64{emit(76);return value}fn upper(value:i64)->i64{emit(72);return value}
fn run<F:Fn(i64)->i32>(f:&F)->i32{let index:i64=7;var total=0;for index in lower(9223372036854775805)..upper(9223372036854775807){total+=f(index);continue}emit(i32(index)+48);return total}
fn main()->i32{let f=move(index:i64)=>{if index==9223372036854775805{emit(65)}else{emit(66)}return 1};return run(&f)-2}
