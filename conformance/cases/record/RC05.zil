resource Token { code: i32 }
package Owner { manages token: Token }
fn bad(value: Owner) -> Owner {
    return Owner { ...value }
}
