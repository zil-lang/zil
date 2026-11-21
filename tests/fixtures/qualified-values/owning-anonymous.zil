extern "C" fn putchar(value:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
fn identity<T:Value>(value:T)->T{return value}
fn sum<life a>(input:&'a i32)->i32{
 let pair=identity::<(Managed<Ticket>,&'a i32)>((Ticket{code:65},input))
 let (ticket,view)=pair
 return *view
}
fn main()->i32{var input:i32=42;return sum(&input)-42}
