pub enum Token { Empty, Byte(u8), Pair(u8, u8) }
pub struct Rule { pub fallback: Token; pub table: [Token; 2] }
pub enum Plan { None, Ready(Rule) }

pub fn prepare() -> Plan {
    var rows = [Token.Empty(), Token.Empty()]
    var index: i64 = 0
    while index < 2 {
        if index == 0 { rows[index] = Token.Pair(u8(1), u8(2)) }
        else { rows[index] = Token.Byte(u8(40)) }
        index += 1
    }
    return Plan.Ready(Rule { fallback: Token.Empty(), table: rows })
}
