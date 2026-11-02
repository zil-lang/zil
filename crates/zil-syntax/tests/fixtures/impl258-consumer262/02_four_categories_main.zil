import std.syntax as syntax
import common.io as io
import generators as gen

#emit gen.answer(syntax.export("answer"))
fn main()->i32 {
    let tmp=90;
    #emit gen.twice(quote<StmtList>{ let tmp=75; io.emit(tmp); })
    io.emit(tmp);
    let value=#splice(gen.plus_one(quote<Expr>{41}));
    let values:#splice(gen.array_spelling(i32,2))=[value,answer()];
    io.emit(48+values[0]/10);
    io.emit(48+values[0]%10);
    return values[1]-42
}
