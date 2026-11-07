import a;
import b;
extern "C" fn getchar()->i32;
fn main()->i32 {
    var left=a.Meter{n:1};
    var right=b.Meter{n:2};
    var input:i32=0;
    unsafe{input=getchar();}
    if input==49 {
        a.run(&mut left);
        let before=left.n;
        let after=a.replace_it(&mut left,30);
        return before+after+left.n+right.n
    } else {
        a.run(&mut right);
        let before=right.n;
        let after=a.replace_it(&mut right,50);
        return before+after+left.n+right.n
    }
}
