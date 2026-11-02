import std.syntax as syntax
import providers.support as support
pub fn expand(input:TokenTree)->Syntax<Expr> {
    let expression=syntax.parse_expr(input);
    return quote<Expr>{ support.bump_runtime($(expression)) }
}
