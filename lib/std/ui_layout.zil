// Pixel geometry is bounded to signed 32-bit positions and nonnegative 31-bit
// extents. Intermediate arithmetic uses i64, including exclusive right/bottom.
pub struct Rect { pub x:i64; pub y:i64; pub width:i64; pub height:i64 }
pub struct Track { pub min:i64; pub max:i64; pub flex:bool }
pub enum Error { Invalid, Space, Output }
fn smaller(a:i64,b:i64)->i64 { if a<b{return a};return b }
fn larger(a:i64,b:i64)->i64 { if a>b{return a};return b }
pub fn valid(rect:Rect)->bool {
    return rect.x>=-2147483648 && rect.x<=2147483647 && rect.y>=-2147483648 && rect.y<=2147483647 && rect.width>=0 && rect.width<=2147483647 && rect.height>=0 && rect.height<=2147483647 && rect.x+rect.width<=2147483647 && rect.y+rect.height<=2147483647
}
pub fn contains(rect:Rect,x:i64,y:i64)->bool {
    return valid(rect) && x>=rect.x && x<rect.x+rect.width && y>=rect.y && y<rect.y+rect.height
}
pub fn intersect(a:Rect,b:Rect)->Error!Rect {
    if !valid(a) || !valid(b){return Error.Invalid}
    let x=larger(a.x,b.x);let y=larger(a.y,b.y)
    let right=smaller(a.x+a.width,b.x+b.width)
    let bottom=smaller(a.y+a.height,b.y+b.height)
    return Rect{x:x,y:y,width:larger(0,right-x),height:larger(0,bottom-y)}
}
// Sum of flexible growth at one water level. Stop at the caller's limit before
// addition; all values are nonnegative and the subtraction is safe.
fn growth(tracks:&[Track],level:i64,limit:i64)->i64 {
    var total:i64=0;var i:i64=0
    while i<tracks.len(){
        let track=tracks[i]
        if track.flex {
            let amount=smaller(track.max-track.min,level)
            if amount>limit-total{return limit+1}
            total+=amount
        }
        i+=1
    }
    return total
}
fn arrange(bounds:Rect,tracks:&[Track],gap:i64,output:&mut [Rect],vertical:bool)->Error!void {
    if !valid(bounds) || gap<0 || gap>2147483647{return Error.Invalid}
    if output.len()!=tracks.len(){return Error.Output}
    let count=tracks.len()
    if count==0{return ()}
    var available=bounds.width
    if vertical{available=bounds.height}
    if count>1 {
        if gap>available/(count-1){return Error.Space}
        available-=(count-1)*gap
    }
    var minimum:i64=0;var i:i64=0
    while i<count {
        let track=tracks[i]
        if track.min<0 || track.max<track.min || track.max>2147483647{return Error.Invalid}
        if track.min>available-minimum{return Error.Space}
        minimum+=track.min;i+=1
    }
    let spare=available-minimum
    var low:i64=0;var high=spare
    while low<high {
        let mid=low+(high-low+1)/2
        if growth(tracks,mid,spare)<=spare{low=mid}else{high=mid-1}
    }
    var remaining=spare-growth(tracks,low,spare)
    var offset:i64=0;i=0
    while i<count {
        let track=tracks[i];var extent=track.min
        if track.flex {
            extent+=smaller(track.max-track.min,low)
            if remaining>0 && extent<track.max{extent+=1;remaining-=1}
        }
        if vertical {output[i]=Rect{x:bounds.x,y:bounds.y+offset,width:bounds.width,height:extent}}
        else {output[i]=Rect{x:bounds.x+offset,y:bounds.y,width:extent,height:bounds.height}}
        offset+=extent
        if i<count-1{offset+=gap}
        i+=1
    }
    return ()
}
pub fn row(bounds:Rect,tracks:&[Track],gap:i64,output:&mut [Rect])->Error!void {return arrange(bounds,tracks,gap,output,false)}
pub fn column(bounds:Rect,tracks:&[Track],gap:i64,output:&mut [Rect])->Error!void {return arrange(bounds,tracks,gap,output,true)}
