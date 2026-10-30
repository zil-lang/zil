extern "C" fn putchar(c:i32)->i32;
fn emit(c:i32){unsafe{putchar(c)}}
resource Token{code:i32;value:i32}
impl Drop for Token{fn drop(self:Self){emit(.code)}}
package Leaf{manages owned:Token;weight:i32}
package Pair{left:Leaf;right:Leaf;tag:i32}
fn leaf(p:&Leaf)->i32{return p.weight+p.owned.value}
fn both(p:&Pair,turn:bool)->i32{let alias=p;var n=0;for i in 0..2{if turn{n+=leaf(&alias.left)}else{n+=leaf(&alias.right)}}return n+alias.tag}
fn main()->i32{let pair=Pair{left:Leaf{owned:Token{code:65,value:5},weight:7},right:Leaf{owned:Token{code:66,value:11},weight:13},tag:3};let p=&pair.left;let n=leaf(p);let child=pair.left;emit(88);return n+child.weight+pair.tag-22}
