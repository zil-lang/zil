struct Phantom<T:Copy>{n:i32}
type First=for<life a> for<life b> fn(&'a i32,&'b i32)->&'a i32;
fn choose(t:Type)->Phantom<First>{return Phantom::<First>{n:42}}
fn result()->i32{let value=#(choose(i32));return value.n}
