
extern "C" fn putchar(n:i32)->i32;
resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages r:R;pub n:i32}
package Q{lead:u8;manages r:R;pub wide:i64}
enum E{Fail}
fn p(code:i32)->P=>P{r:R{code:code},n:7}
fn q(code:i32)->Q=>Q{lead:u8(3),r:R{code:code},wide:123456789}

fn empty<T:StoreElement>()->Store<T> contract{result(create)}=>store_new()
fn make<T:StoreElement>(value:T,fail:bool)->E!Store<T> contract{result(create)}{
 let s:Store<T>=empty();if fail{return E.Fail};
 if !store_reserve(s,1){return E.Fail};store_push(s,value);return give s
}
fn forward<T:StoreElement>(give x:E!Store<T>)->E!Store<T> contract{result(give)}=>x
fn wrap<T:StoreElement>(give x:Store<T>)->E!Store<T> contract{result(give)}=>give x

enum WideError{Bad(i64,u8)}
fn create<T:StoreElement>(fail:bool)->WideError!Store<T>contract{result(create)}{
 if fail{return WideError.Bad(123456789,u8(7))};return store_new()
}
fn main()->WideError!i32{let pending=create::<Q>(true);let store=pending?;return 0}
