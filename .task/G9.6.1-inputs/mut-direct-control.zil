fn value<life a>(p:&'a mut i64)->&'a mut i64{return p}fn main()->i32{var n:i64=40;let q=value(&mut n);*q+=2;return i32(n)}
