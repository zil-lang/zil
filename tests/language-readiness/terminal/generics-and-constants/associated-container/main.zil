import model
import consumer
fn main()->i32{let source=model.make();let holder=consumer.wrap::<model.Source>(&source);let first=consumer.direct(consumer.take::<model.Source>(holder));match consumer.maybe::<model.Source>(&source){Option.Some(second)=>{return first+second-84},Option.None()=>{return 1}}}
