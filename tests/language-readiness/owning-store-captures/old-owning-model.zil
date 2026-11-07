resource R{n:i32} package Item{manages r:R} package Model{manages values:Store<Item>} fn make()->Model{let s:Store<Item>=store_new();store_reserve(s,1);store_push(s,Item{r:R{n:42}});return Model{values:give s}} fn relay(m:Model)->Model=>m;
fn main()->i32{let m=relay(make());return i32(store_len(m.values))}
