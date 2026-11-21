extern "C" fn putchar(value:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
struct View<life a>{value:&'a i32}
fn identity<T:Value>(value:T)->T{return value}
fn sum<life a>(input:&'a i32)->i32{
 let output=identity::<(Managed<Ticket>,View<'a>)>((Ticket{code:65},View::<'a>{value:input}))
 let (ticket,view)=output
 return *view.value
}
fn main()->i32{var input:i32=42;return sum(&input)-42}
