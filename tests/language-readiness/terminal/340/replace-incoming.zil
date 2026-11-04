resource R{code:i32}
package P{manages owner:R;n:i32}
fn replace(target:&mut P,give incoming:R){target.owner=give incoming}
fn relay(target:&mut P,give incoming:R){replace(target,give incoming)}
fn make(give incoming:R)->R contract{result(create)}{
    var packet=P{owner:R{code:65},n:0};
    relay(&mut packet,give incoming);
    return give packet.owner
}
fn result()->i32{let owner=make(R{code:66});return owner.code-24}
