import std.ui_layout as layout
import std.ui_raster as raster
fn attempt(data:&mut[u8],width:i64,height:i64,stride:i64,rect:layout.Rect,color:raster.Color,blend:bool,continued:&mut bool)->raster.Error!void {
    raster.fill(data,width,height,stride,rect,color,blend)?;*continued=true;return ()
}
fn main()->raster.Error!i32 {
    var pixels=[0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8,0u8]
    pixels[12]=99u8;pixels[13]=98u8;pixels[14]=97u8;pixels[15]=96u8
    let full=layout.Rect{x:0,y:0,width:3,height:2}
    raster.fill(&mut pixels[..],3,2,16,full,raster.Color{r:10u8,g:20u8,b:30u8,a:255u8},false)?
    raster.fill(&mut pixels[..],3,2,16,layout.Rect{x:-1,y:-1,width:3,height:3},raster.Color{r:100u8,g:0u8,b:0u8,a:128u8},true)?
    if pixels[0]!=105u8 || pixels[1]!=10u8 || pixels[2]!=15u8 || pixels[3]!=255u8 || pixels[16]!=105u8 || pixels[8]!=10u8{return 1}
    if pixels[12]!=99u8 || pixels[13]!=98u8 || pixels[14]!=97u8 || pixels[15]!=96u8{return 2}
    var continued=false
    attempt(&mut pixels[..27],3,2,16,full,raster.Color{r:0u8,g:0u8,b:0u8,a:255u8},false,&mut continued)
    attempt(&mut pixels[..],3,2,11,full,raster.Color{r:0u8,g:0u8,b:0u8,a:255u8},false,&mut continued)
    attempt(&mut pixels[..],3,2,16,full,raster.Color{r:255u8,g:0u8,b:0u8,a:128u8},true,&mut continued)
    if continued || pixels[0]!=105u8 || pixels[8]!=10u8{return 3}
    pixels[27]=0u8
    attempt(&mut pixels[..],3,2,16,full,raster.Color{r:0u8,g:0u8,b:0u8,a:0u8},true,&mut continued)
    if continued || pixels[0]!=105u8 || pixels[8]!=10u8{return 4}
    raster.fill(&mut pixels[..],3,2,16,layout.Rect{x:100,y:100,width:10,height:10},raster.Color{r:0u8,g:0u8,b:0u8,a:255u8},false)?
    if pixels[0]!=105u8 || pixels[27]!=0u8{return 5}
    return 0
}
