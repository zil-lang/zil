trait Measure{fn length(self:&Self)->i32;}
fn measured(value:&dyn Measure)->i32=>value.length();fn main()->i32=>0
