fn increment(value:i32)->i32 { return value+1 }
trait First { const A:i32; fn marker_first(self:&Self)->i32 { return 0 } const B:i32=increment(Self.A); }
trait Second { const A:i32; fn marker_second(self:&Self)->i32 { return 0 } const B:i32=Self.A+2; }
struct P {}
struct Q {}
impl Second for P { const A:i32=100; }
impl First for Q { const B:i32=70; const A:i32=8; }
impl First for P { const A:i32=40; }
fn first<T:First>()->i32 { return T.B }
fn second<T:Second>()->i32 { return T.B }
fn main()->i32 {
    if first::<P>()!=41 { return 1 }
    if second::<P>()!=102 { return 2 }
    if first::<Q>()!=70 { return 3 }
    return 0
}
