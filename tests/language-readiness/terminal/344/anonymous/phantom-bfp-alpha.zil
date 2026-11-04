struct Phantom<T:Copy>{n:i32}
type First=for<life a> for<life b> fn(&'a i32,&'b i32)->&'a i32;
type Alpha=for<life y> for<life x> fn(&'x i32,&'y i32)->&'x i32;
type Second=for<life a> for<life b> fn(&'a i32,&'b i32)->&'b i32;
fn choose(t:Type)->Phantom<First>{return Phantom::<First>{n:42}}
fn result()->i32{let value:Phantom<Alpha>=#(choose(i32));return value.n}
