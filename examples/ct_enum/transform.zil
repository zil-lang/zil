import model

fn apply(token: model.Token, byte: u8) -> u8 {
    match token {
        model.Token.Empty() => { return byte },
        model.Token.Byte(extra) => { return u8((i64(byte) + i64(extra)) % 256) },
        model.Token.Pair(left, right) => {
            return u8((i64(byte) + i64(left) + i64(right)) % 256)
        }
    }
}

pub fn run(plan: model.Plan, byte: u8) -> u8 {
    match plan {
        model.Plan.None() => { return u8(0) },
        model.Plan.Ready(rule) => {
            let first = apply(rule.fallback, byte)
            return apply(rule.table[i64(byte) % 2], first)
        }
    }
}
