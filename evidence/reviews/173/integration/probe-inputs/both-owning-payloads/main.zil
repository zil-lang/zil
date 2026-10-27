extern "C" fn putchar(value:i32)->i32;
resource Ticket{pub code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package E{manages ticket:Ticket}
package Q{manages ticket:Ticket}
fn choice(fail:bool)->E!Q{if fail{return E{ticket:Ticket{code:68}}};return Q{ticket:Ticket{code:69}}}
fn relay(value:E!Q)->E!Q{return value}
fn keep()->impl Fn()->i32{let c=Ticket{code:67};return move()=>c.code}
fn main()->i32{let callback=keep();relay(choice(true));relay(choice(false));return callback()-67}
