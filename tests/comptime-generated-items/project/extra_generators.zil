import std.syntax as syntax
import common.io as io
pub fn fresh_echo(value:Syntax<Expr>)->Syntax<StmtList> {
    let name=syntax.fresh("tmp");
    let reference=syntax.name_expr(name);
    return quote<StmtList>{ let $(name)=$(value); io.emit($(reference)); }
}
pub fn type_shadow(t:Type)->Type {
    let i32=t;
    return i32
}
pub fn exported_name_is_not_input()->Syntax<Expr> {
    return syntax.name_expr(syntax.export("private_local"))
}
