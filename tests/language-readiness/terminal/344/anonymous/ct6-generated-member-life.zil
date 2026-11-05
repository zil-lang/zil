import std.syntax as syntax;
struct Phantom<life s>{n:i32}
fn scalar(t:Type,n:i32)->i32{return n}
#emit quote<ItemList>{
    struct $(syntax.export("Reader")) {
        fn result<life s>(self:Self,p:&'s i32)->i32 {
            let value=#(Phantom::<'s>{n:scalar(i32,42)});
            return value.n
        }
    }
};
fn main()->i32{let n=0;let reader=Reader{};return reader.result(&n)}
