import std.ui_layout as layout
fn bad(bounds:layout.Rect,tracks:&[layout.Track],gap:i64,output:&mut [layout.Rect],continued:&mut bool)->layout.Error!void {
    layout.row(bounds,tracks,gap,output)?;*continued=true;return ()
}
fn main()->layout.Error!i32 {
    let bounds=layout.Rect{x:10,y:20,width:103,height:50}
    let tracks=[layout.Track{min:10,max:10,flex:false},layout.Track{min:10,max:20,flex:true},layout.Track{min:10,max:100,flex:true}]
    let zero=layout.Rect{x:0,y:0,width:0,height:0}
    var output=[zero,zero,zero]
    layout.row(bounds,&tracks[..],2,&mut output[..])?
    if output[0].x!=10 || output[0].width!=10 || output[1].x!=22 || output[1].width!=20 || output[2].x!=44 || output[2].width!=69 || output[2].height!=50{return 1}
    if !layout.contains(output[2],112,69) || layout.contains(output[2],113,69) || layout.contains(output[2],112,70){return 2}
    let clip=layout.intersect(output[2],layout.Rect{x:100,y:10,width:30,height:20})?
    if clip.x!=100 || clip.y!=20 || clip.width!=13 || clip.height!=10{return 3}
    let empty=layout.intersect(output[0],layout.Rect{x:200,y:200,width:1,height:1})?
    if empty.width!=0 || empty.height!=0{return 4}
    var continued=false
    bad(bounds,&tracks[..],60,&mut output[..],&mut continued)
    bad(bounds,&tracks[..],-1,&mut output[..],&mut continued)
    bad(bounds,&tracks[..],0,&mut output[..2],&mut continued)
    if continued || output[0].x!=10 || output[1].width!=20 || output[2].width!=69{return 5}
    layout.column(bounds,&tracks[..],2,&mut output[..])?
    if output[0].y!=20 || output[1].y!=32 || output[1].height!=18 || output[2].y!=52 || output[2].height!=18{return 6}
    let flexible=[layout.Track{min:0,max:2147483647,flex:true},layout.Track{min:0,max:2147483647,flex:true}]
    layout.row(layout.Rect{x:0,y:0,width:2147483647,height:1},&flexible[..],0,&mut output[..2])?
    if output[0].width!=1073741824 || output[1].width!=1073741823 || output[1].x!=1073741824{return 7}
    let invalid=[layout.Track{min:20,max:10,flex:true}]
    bad(bounds,&invalid[..],0,&mut output[..1],&mut continued)
    bad(layout.Rect{x:2147483647,y:0,width:1,height:1},&invalid[..],0,&mut output[..1],&mut continued)
    if continued || output[0].width!=1073741824{return 8}
    return 0
}
