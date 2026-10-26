pub fn add(x:i64)->i64=>x+2
fn double(x:i64)->i64=>x*2
pub fn choose(first:bool)->fn(i64)->i64=>if first {add} else {double}
