import std.syntax as syntax
import common.io as io
import extra_generators as extra
syntax use providers.expressions as inc

type Selected=#(extra.type_shadow(i64))
fn main()->i32 {
    let typed:Selected=i64(7);
    let tmp=90;
    #emit extra.fresh_echo(quote<Expr>{75})
    #emit extra.fresh_echo(quote<Expr>{75})
    io.emit(tmp);
    if typed!=i64(7) { return 1 }
    return inc.named!(41)-42
}
