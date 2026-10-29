import io
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){io.emit(.code)}}

pub fn make(seed:i64,code:i32,stop:i64)->impl FnMut(i64)->bool!i64{
    let ticket=Ticket{code:code};
    var total=seed;
    return move (delta:i64)->bool!i64=>{
        let observed=ticket.code;
        total+=delta;
        if delta==stop{return true}
        return total
    }
}
