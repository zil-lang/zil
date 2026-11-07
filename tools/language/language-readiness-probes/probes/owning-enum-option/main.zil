resource R{n:i32}
package Leaf{manages owner:R}
enum Maybe{Empty,Some(Leaf)}
fn main(){let v=Maybe.Some(Leaf{owner:R{n:3}})}
