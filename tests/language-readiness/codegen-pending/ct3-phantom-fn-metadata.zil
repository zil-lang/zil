struct Phantom<T:Copy>{n:i32}
fn choose(t:Type)->Phantom<fn(i32)->i32>{return Phantom::<fn(i32)->i32>{n:7}}
fn result()->i32{let value=#(choose(i32));return value.n}
