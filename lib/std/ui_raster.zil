import std.ui_layout as layout

pub struct Color { pub r:u8; pub g:u8; pub b:u8; pub a:u8 }
pub enum Error { Invalid, Buffer }
// RGBA bytes, row-major, with an explicit stride. Transparent source-over uses
// premultiplied channels; r/g/b must be <= a. Row padding is never touched.
fn valid_color(color:Color)->bool {return color.r<=color.a && color.g<=color.a && color.b<=color.a}
fn valid_buffer(length:i64,width:i64,height:i64,stride:i64)->bool {
    if width<0 || width>2147483647 || height<0 || height>2147483647 || stride<0{return false}
    let row=width*4
    if stride<row{return false}
    if height==0 || width==0{return true}
    if row>length{return false}
    return height-1==0 || stride<=(length-row)/(height-1)
}
fn channel(source:u8,destination:u8,alpha:u8)->u8 {
    return u8(i32(source)+(i32(destination)*(255-i32(alpha))+127)/255)
}
// Validate all public inputs before mutation. Clip rectangles to the surface;
// an empty intersection succeeds without touching any pixel or padding byte.
pub fn fill(data:&mut [u8],width:i64,height:i64,stride:i64,rect:layout.Rect,color:Color,blend:bool)->Error!void {
    if !layout.valid(rect) || !valid_color(color){return Error.Invalid}
    if !valid_buffer(data.len(),width,height,stride){return Error.Buffer}
    var left=rect.x;var top=rect.y
    var right=rect.x+rect.width;var bottom=rect.y+rect.height
    if left<0{left=0};if top<0{top=0}
    if right>width{right=width};if bottom>height{bottom=height}
    if left>=right || top>=bottom{return ()}
    // Blending requires a valid premultiplied destination. Check the complete
    // affected region first so malformed input never leaves a partial drawing.
    if blend {
        var y=top
        while y<bottom {
            var x=left
            while x<right {
                let at=y*stride+x*4;let a=data[at+3]
                if data[at]>a || data[at+1]>a || data[at+2]>a{return Error.Invalid}
                x+=1
            }
            y+=1
        }
    }
    var y=top
    while y<bottom {
        var x=left
        while x<right {
            let at=y*stride+x*4
            if blend {
                data[at]=channel(color.r,data[at],color.a)
                data[at+1]=channel(color.g,data[at+1],color.a)
                data[at+2]=channel(color.b,data[at+2],color.a)
                data[at+3]=channel(color.a,data[at+3],color.a)
            } else {
                data[at]=color.r;data[at+1]=color.g;data[at+2]=color.b;data[at+3]=color.a
            }
            x+=1
        }
        y+=1
    }
    return ()
}
