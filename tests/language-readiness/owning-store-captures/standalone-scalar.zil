fn main()->i32{let s:Store<i32>=store_new();store_reserve(s,1);store_push(s,42);return i32(store_len(s))}
