import component
fn read(panel:&component.Panel)->i32 { return panel.count }
fn main()->i32 {
    var panel=component.open()
    let view=&panel
    component.apply(&mut panel,1)
    return read(view)
}
