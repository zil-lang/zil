package Model{manages values:Store<i32>} fn make()->Model{let s:Store<i32>=store_new();store_reserve(s,1);store_push(s,42);return Model{values:give s}} fn relay(m:Model)->Model=>m;
fn main()->i32{let m=relay(make());return i32(store_len(m.values))}
