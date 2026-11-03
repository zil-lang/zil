struct Counter{count:i32}
fn bump(counter:&mut Counter){counter.count+=1}
fn main()->i32{var counter=Counter{count:0};bump(&mut counter);return counter.count-1}
