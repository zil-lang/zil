struct Phantom<T:Copy>{n:i32}
struct Code{call:fn(i32)->i32}
fn record(t:Type)->Phantom<Code>{return Phantom::<Code>{n:14}}
fn tuple(t:Type)->Phantom<(fn(i32)->i32,i32)>{return Phantom::<(fn(i32)->i32,i32)>{n:14}}
fn vacuous(t:Type)->Phantom<for<life unused> fn(i32)->i32>{return Phantom::<for<life unused> fn(i32)->i32>{n:14}}
fn result()->i32{let a=#(record(i32));let b=#(tuple(i32));let c=#(vacuous(i32));return a.n+b.n+c.n}
