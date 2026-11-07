struct Phantom<T:Copy>{n:i32}
fn result()->i32{let value:Phantom<fn(i32)->i32>=Phantom::<fn(i32)->i32>{n:7};return value.n}
