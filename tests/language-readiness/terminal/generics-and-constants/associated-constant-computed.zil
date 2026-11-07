fn calculate(x:i64)->i64{return x*7}
trait Wide{const N:(((i64)))=(((calculate(1000000000))));const TOP:u64;}
struct P{}
impl Wide for P{const TOP:(((u64)))=(((18446744073709551615u64-1u64)));}
fn number<T:Wide>()->i64{return T.N}
fn top<T:Wide>()->u64{return T.TOP}
fn main()->i32{
    if (((P))).TOP!=top::<P>(){return 1}
    if top::<P>()!=18446744073709551614u64{return 1}
    return i32(number::<P>()-7000000000)
}
