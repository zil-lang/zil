resource Token { value:i32 }
fn read(t:Token)->i32 {return t.value}
fn main()->i32 {let t=Token{value:7};let alias=t;var n:i32=0;while n<4 {n+=1;if n==2 {continue}if n==3 {break}read(alias)}return read(alias)+n}
