import rules;
import model;
fn main()->i32{let p=model.P{};let pointer=rules.select(&p);return pointer(7)-21}
