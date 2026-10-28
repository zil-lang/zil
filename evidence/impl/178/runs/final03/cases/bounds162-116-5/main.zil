
trait Measure { fn measure(self:&Self)->i32; }
struct Point { value:i32 }
struct Area { width:i32,height:i32 }
impl Measure for Point { fn measure()->i32=>.value }
impl Measure for Area { fn measure()->i32=>.width*.height }
fn measured<T:Measure>(value:&T)->i32=>value.measure()
fn relay<U:Measure>(value:&U)->i32=>measured(value)
fn main()->i32 { let p=Point{value:7};let a=Area{width:5,height:7};return relay(&p)+measured::<Area>(&a)-42 }
