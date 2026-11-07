import provider
import consumer

fn main()->i32{
    let box=provider.make(42);
    match consumer.relay(&box){
        Option.Some(value)=>{return provider.number(value)-42},
        Option.None()=>{return 1}
    }
}
