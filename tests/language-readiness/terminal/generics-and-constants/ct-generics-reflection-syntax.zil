import std.meta as meta;
import std.syntax as syntax;
struct Config<T:Copy,P:Copy>{value:T,n:i32}
fn value_type<T:Copy>(x:T)->Type{return T}
fn field_type<T:Copy>(x:T)->Type{return meta.field_type(meta.field(T,"value"))}
fn type_syntax<T:Copy>(x:T)->Syntax<Type>{return syntax.type_ref(T)}
fn expression<T:Copy,P:Copy>(x:Config<T,P>)->Syntax<Expr>{
    return quote<Expr>{$(syntax.literal(x.n))}
}
fn main()->i32{
    let wide:#(field_type(Config::<i64,bool>{value:7,n:7}))=3000000000;
    let config:#splice(type_syntax(Config::<i32,bool>{value:7,n:7}))=Config::<i32,bool>{value:7,n:7};
    let number:#(value_type(7))=#splice(expression(Config::<i32,bool>{value:7,n:7}));
    return number+config.value-14+i32(wide-3000000000)
}
