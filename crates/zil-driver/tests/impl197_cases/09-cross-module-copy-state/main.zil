import model
import state
fn main()->i32{let callback=move(value:model.Data)=>model.Data{n:value.n+1};let current=state.make(callback,model.make());let answer=state.run(current);return answer.n-42}
