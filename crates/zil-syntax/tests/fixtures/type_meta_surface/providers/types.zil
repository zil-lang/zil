import std.syntax as syntax
pub fn expand(input:TokenTree)->Syntax<Type> {
    let element=syntax.parse_type(input);
    return quote<Type>{ [$(element);4] }
}
