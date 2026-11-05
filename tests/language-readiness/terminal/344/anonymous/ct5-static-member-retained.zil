struct Phantom<life a,T:Copy,#N:i32>{n:i32}
struct Maker {
    fn make<T:Copy,#N:i32,life a>(marker:Type)->Phantom<'a,T,N>{return Phantom::<'a,T,N>{n:N}}
    fn read<life b>(self:Self,p:&'b i32)->i32{
        let value:Phantom<'b,i32,7>=#(Phantom::<'b,i32,7>{n:7});
        return value.n
    }
}
fn result<life s>(p:&'s i32)->i32{
    let value:Phantom<'s,i32,35>=#(Maker.make::<i32,35,'s>(i32));
    let maker=Maker{};
    return value.n+maker.read(p)
}
fn main()->i32{let n=0;return result(&n)}
