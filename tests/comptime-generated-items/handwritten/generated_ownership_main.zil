import common.model as model
pub package GeneratedOwner { manages ticket:model.Ticket; pub n:i32 }
pub fn run_generated(early:bool)->i32 {
    let first=model.Ticket{code:65};
    if early { return 0 }
    let second=model.Ticket{code:66};
    return 0
}
pub fn never_called(early:bool)->i32 {
    let first=model.Ticket{code:65};
    if early { return 0 }
    let second=model.Ticket{code:66};
    return 0
}
fn main()->i32 {
    run_generated(false);
    run_generated(true);
    let ticket=model.Ticket{code:80};
    let owner=GeneratedOwner{ticket:give ticket,n:7};
    return owner.n-7
}
