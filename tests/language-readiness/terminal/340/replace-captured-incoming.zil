resource R{code:i32}
package P{manages owner:R;n:i32}
fn make(give incoming:R)->R contract{result(create)}{
    var pair=(P{owner:R{code:65},n:0},0);
    let target=&mut pair;
    let replace=move()->void=>{let(ref mut packet,_)=*target;packet.owner=give incoming};
    replace();
    let(packet,_)=pair;
    return give packet.owner
}
fn result()->i32{let owner=make(R{code:66});return owner.code-24}
