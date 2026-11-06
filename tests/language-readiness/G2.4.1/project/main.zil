import api
import cursor
import consumer
import other

fn main()->i32{
    var numbers=cursor.make(20,22);
    var sum=0;
    match consumer.next_copy(&mut numbers){
        Option.Some(value)=>{sum+=value},Option.None()=>{return 1}
    }
    match consumer.next_copy(&mut numbers){
        Option.Some(value)=>{sum+=value},Option.None()=>{return 2}
    }
    if sum!=42{return 3}
    match consumer.next_copy(&mut numbers){
        Option.None()=>{},Option.Some(_)=>{return 4}
    }
    var tokens=other.make();
    match consumer.next_copy(&mut tokens){
        Option.Some(value)=>{if other.number(value)!=7{return 5}},
        Option.None()=>{return 6}
    }
    match consumer.next_copy(&mut tokens){
        Option.Some(value)=>{if other.number(value)!=11{return 7}},
        Option.None()=>{return 8}
    }
    match consumer.next_copy(&mut tokens){
        Option.None()=>{},Option.Some(_)=>{return 9}
    }
    return sum
}
