resource R{x:i32}
package P{manages r:R}
fn failed()->P!void{return P{r:R{x:7}}}
fn relay(value:P!void)->P!void{return value}
fn kept()->impl Fn()->i32{let n=7;return move()=>n}
fn main()->i32{let error=failed();let f=()=>{error;return 0};return f()}
