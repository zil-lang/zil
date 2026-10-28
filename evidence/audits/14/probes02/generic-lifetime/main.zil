fn identity<life a>(value:&'a i32)->&'a i32=>value;fn main()->i32{let n=7;return *identity(&n)-7}
