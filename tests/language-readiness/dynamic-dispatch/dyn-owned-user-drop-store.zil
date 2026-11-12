extern "C" fn putchar(code:i32)->i32;
trait Read{fn read(self:&Self)->i32;}
resource Token{code:i32}
impl Drop for Token{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages mark:Token;pub n:i32}
package A{manages mark:Token;pub n:i32}
package B{manages items:Store<Item>;pub n:i32}
impl Read for A{fn read(self:&Self)->i32{return self.n}}
impl Read for B{fn read(self:&Self)->i32{return self.n}}
fn first()->DynCreateError!i32{
    let owner=owned_dyn::<dyn Read>(A{mark:Token{code:65},n:19})?;
    return consume(owner)
}
fn second()->DynCreateError!i32{
    let items:Store<Item>=store_new();
    store_reserve(items,2);
    store_push(items,Item{mark:Token{code:66},n:2});
    store_push(items,Item{mark:Token{code:67},n:3});
    let owner=owned_dyn::<dyn Read>(B{items:give items,n:23})?;
    return consume(owner)
}
fn consume(owner:OwnedDyn<dyn Read>)->i32{return owner.read()}
fn run()->DynCreateError!i32{return first()?+second()?}
fn main()->i32{
    let result=run();
    match result{.ok(value)=>{return value},.err(_)=>{return 99}}
}
