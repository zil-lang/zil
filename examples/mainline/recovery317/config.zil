import std.syntax as syntax

pub struct Config<T:Copy,P:Copy> { pub value:T }
pub fn value_type<T:Copy>(value:T)->Type { return T }
pub fn expression(config:Config<i32,bool>)->Syntax<Expr> {
    return quote<Expr>{ $(syntax.literal(config.value)) }
}
pub fn wide_expression(config:Config<i64,i32>)->Syntax<Expr> {
    return quote<Expr>{ $(syntax.literal(config.value)) }
}
