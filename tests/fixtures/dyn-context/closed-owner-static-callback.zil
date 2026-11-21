struct Value{number:i32}
trait Read{fn read(self:&Self)->i32;}
impl Read for Value{fn read(self:&Self)->i32{return self.number}}
fn keep<F:Fn()->i32>(input:F)->F{return input}
fn main()->i32{
    let text="abc";
    match owned_dyn::<dyn Read>(Value{number:20}){
        .err(_)=>{return 99},
        .ok(owner)=>{
            let callback=keep(move()=>owner.read()+i32(text[0]));
            return callback()+callback()-234;
        }
    }
}
