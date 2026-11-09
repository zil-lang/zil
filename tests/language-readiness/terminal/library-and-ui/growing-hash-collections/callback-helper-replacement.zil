resource Token { n:i64 }
package Holder { manages marker:Token }
fn replace(holder:&mut Holder) { holder.marker=Token{n:22} }
fn invoke<F:FnMut()->void>(action:&mut F) { action() }
fn forward<F:FnMut()->void>(action:&mut F) { invoke(action) }
fn main()->i32 {
    var holder=Holder{marker:Token{n:11}}
    let old=holder.marker
    if old.n!=11 {return 2}
    let target=&mut holder
    var action=()=>{ replace(target) }
    forward(&mut action)
    let current=holder.marker
    return i32(current.n)-22
}
