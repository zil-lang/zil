import cursor
import consumer
fn main()->i32{var numbers=cursor.make(20,22);let first=consumer.capture::<cursor.ArrayCursor<i32>>(&mut numbers);let second=consumer.capture::<cursor.ArrayCursor<i32>>(&mut numbers);match consumer.take::<i32>(first){Option.Some(a)=>{match consumer.take::<i32>(second){Option.Some(b)=>{return a+b-42},Option.None()=>{return 2}}},Option.None()=>{return 1}}}
