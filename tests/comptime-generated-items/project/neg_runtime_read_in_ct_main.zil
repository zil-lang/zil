import common.model as model
fn main()->i32 {
    let ticket=model.Ticket{code:82};
    return #(ticket.code)
}
