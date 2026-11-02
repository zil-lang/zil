import std.syntax as syntax
import providers.support as support
pub fn expand(input:TokenTree)->Syntax<StmtList> {
    let expression=syntax.parse_expr(input);
    return quote<StmtList>{
        let tmp=$(expression);
        support.emit(tmp);
        support.emit(tmp);
    }
}
