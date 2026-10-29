resource R{n:i32}package P{manages r:R}fn main()->i32{let s:Store<P>=store_new();let f=()=>store_len(s);return i32(f())}
