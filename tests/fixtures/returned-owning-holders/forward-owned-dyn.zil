extern "C" fn putchar(value:i32)->i32;
trait Read {fn read(self:&Self)->i32;}
resource Ticket {code:i32}
impl Drop for Ticket {fn drop(self:Self){unsafe{putchar(.code)}}}
package Object {manages ticket:Ticket; value:i32}
impl Read for Object {fn read(self:&Self)->i32{return self.value}}
fn make(owner:OwnedDyn<dyn Read>,pointer:&i32)->impl Fn()->i32{return move()=>owner.read()+*pointer}
fn keep<F:Fn()->i32>(input:F)->F{return input}
fn main()->i32 {
    let number=22
    match owned_dyn::<dyn Read>(Object{ticket:Ticket{code:65},value:20}) {
        .err(_)=>{return 1},
        .ok(owner)=>{let callback=keep(make(owner,&number));return callback()-42}
    }
}
