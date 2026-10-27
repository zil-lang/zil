import model
pub fn keep()->impl FnMut()->i32 {
    let ticket=model.Ticket{code:67}
    var value=0
    return move ()->i32=>{value+=1;return value+ticket.code-67}
}
