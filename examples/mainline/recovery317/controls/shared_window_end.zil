import component
fn read(panel:&component.Panel)->i32 { return panel.count }
fn main()->i32 {
    var panel=component.open()
    let view=&panel
    let prior=read(view)
    component.apply(&mut panel,1)
    return panel.count-prior-1
}
