import std.syntax as syntax
pub fn expand(input:TokenTree)->Syntax<Expr> {
    let expression=syntax.parse_expr(input);
    return quote<Expr>{ $(expression)*2 }
}
