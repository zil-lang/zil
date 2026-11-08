import protocol
import library
import model
fn main()->i32 {
    match model.make() {
        .err(_)=>{return 1},
        .ok(original)=>{
            let copy=library.duplicate(&original)
            if model.size(&copy)!=1 {return 2}
            if copy.tag!=8 || model.byte(&copy)!=65u8 {return 3}
            match library.try_duplicate(&original,true) {
                .err(error)=>{match error{protocol.Error.Requested()=>{},protocol.Error.Allocation()=>{return 4}}},
                .ok(value)=>{return 5}
            }
            match library.try_duplicate(&original,false) {
                .err(_)=>{return 6},
                .ok(second)=>{
                    if second.tag!=9 || model.byte(&second)!=65u8 {return 7}
                    if original.tag!=7 || model.byte(&original)!=65u8 {return 8}
                }
            }
        }
    }
    return 0
}
