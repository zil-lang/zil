import std.syntax as syntax
pub fn expand(input:TokenTree)->Syntax<ItemList> {
    let name=syntax.parse_ident(input);
    return quote<ItemList>{ pub fn $(name)()->i32 { return 7 } }
}
