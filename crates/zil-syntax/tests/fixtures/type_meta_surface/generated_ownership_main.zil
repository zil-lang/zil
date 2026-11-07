import std.syntax as syntax
import common.model as model
import generators as gen

#emit gen.owned_package(syntax.export("GeneratedOwner"))
#emit gen.cleanup(syntax.export("run_generated"))
#emit gen.cleanup(syntax.export("never_called"))
fn main()->i32 {
    run_generated(false);
    run_generated(true);
    let ticket=model.Ticket{code:80};
    let owner=GeneratedOwner{ticket:give ticket,n:7};
    return owner.n-7
}
