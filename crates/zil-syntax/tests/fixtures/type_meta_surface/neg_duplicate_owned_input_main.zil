import common.model as model
import generators as gen
fn main()->i32 {
    let ticket=model.Ticket{code:82};
    #emit gen.duplicate(quote<Expr>{give ticket})
    return 0
}
