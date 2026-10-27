import model;import factory;import update;
fn main()->i32{
    var state=model.wrap(factory.counter(5,65));
    state=update.step(state,3);state=update.twice(state,2);
    if state.seen!=12||state.lead[2]!=23i64||state.tail!=513u16{return 10}
    let first=&mut state.callback;if first(1)!=13{return 11}
    state.callback=factory.counter(20,66);
    let second=&mut state.callback;if second(2)!=22{return 12}
    if state.lead[0]+state.lead[1]!=36i64{return 13}
    return 0
}