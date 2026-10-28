import helpers
fn main()->i32{let n=3;let f=(p:&i32)->i32=>3;return helpers.apply(&f,&n)-3}
