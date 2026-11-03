trait Measure {fn measure(self:&Self)->i32;}
struct Point {value:i32}
impl Measure for Point {fn measure()->i32=>.value}
fn measured<T:Measure>(value:&T)->i32=>value.measure()
fn main()->i32{let p=Point{value:7};return measured(&p)-7}
