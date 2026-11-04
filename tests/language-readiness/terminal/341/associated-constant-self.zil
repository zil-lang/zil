fn increment(value:i32)->i32 { return value+1 }
trait First { const A:i32; const B:i32=increment(Self.A); }
trait Second { const A:i32; const B:i32=Self.A+2; }
struct P {}
struct Q {}
impl Second for P { const A:i32=100; }
impl First for Q { const B:i32=70; const A:i32=8; }
impl First for P { const A:i32=40; }
fn first<T:First>()->i32 { return T.B }
fn second<T:Second>()->i32 { return T.B }
fn main()->i32 {
    return first::<P>()+second::<P>()+first::<Q>()-213
}
