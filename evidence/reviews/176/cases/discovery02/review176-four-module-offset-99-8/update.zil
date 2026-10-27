
import model
pub fn step<G:FnMut(i32)->i32>(state:model.Scene<G>,amount:i32)->model.Scene<G>{
    var next=state;let callback=&mut next.callback;next.seen=callback(amount);return next
}
pub fn twice<H:FnMut(i32)->i32>(state:model.Scene<H>,amount:i32)->model.Scene<H>{return step(step(state,amount),amount)}
