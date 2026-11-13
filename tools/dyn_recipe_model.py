"""Pure Rust data metamodel shared by the recipe/CT generators.

Semantic original bodies specialize the actual Core AST declarations. This
module owns no second AST schema and performs no output or semantic checking.
"""
import re

# Filled from the public aliases, not a second list of Core operation types.
# The dictionary is mutated in place so the CT generator sees the same aliases.
ORIGINAL_ALIASES = {}


def original_aliases(path):
    source = re.sub(r'//[^\n]*', '', path.read_text())
    return {match.group(1): match.group(2).strip()
            for match in re.finditer(
                r'pub type (SemanticOriginal\w+)\s*=\s*([^;]+);', source)}


def split(source):
    result = []
    start = depth = 0
    for index, char in enumerate(source):
        if char in '<({[':
            depth += 1
        elif char in '>)}]':
            depth -= 1
        elif char == ',' and depth == 0:
            result.append(source[start:index].strip())
            start = index + 1
    if source[start:].strip():
        result.append(source[start:].strip())
    return result


def canonical_type(ty):
    for alias, value in ORIGINAL_ALIASES.items():
        ty = re.sub(r'\b' + alias + r'\b', value, ty)
    return re.sub(r'\s+', '', ty)


def tuple_parts(ty):
    ty = canonical_type(ty)
    return split(ty[1:-1]) if ty.startswith('(') and ty.endswith(')') else None


def fields(source):
    result = []
    for field in split(source):
        name, ty = field.split(':', 1)
        result.append((name.replace('pub ', '').strip(), canonical_type(ty)))
    return result


def block(source, start):
    depth = 1
    index = start
    while depth:
        if source[index] == '{':
            depth += 1
        elif source[index] == '}':
            depth -= 1
        index += 1
    return source[start:index - 1]


def declarations(path, only=None):
    source = re.sub(r'//[^\n]*', '', path.read_text())
    result = {}
    # These are data declarations, including Core's Annotation=Type parameter;
    # functions/impls and their executable statements never enter the grammar.
    pattern = r'pub (struct|enum) (\w+)(?:\s*<[^;{}]*>)?\s*\{'
    for match in re.finditer(pattern, source):
        kind, name = match.group(1, 2)
        if only is not None and name not in only:
            continue
        body = block(source, match.end())
        if kind == 'struct':
            result[name] = kind, fields(body)
            continue
        rows = []
        for value in split(body):
            if '{' in value:
                variant, raw = value.split('{', 1)
                rows.append((variant.strip(), 'struct', fields(raw.rsplit('}', 1)[0])))
            elif '(' in value:
                variant, raw = value.split('(', 1)
                rows.append((variant.strip(), 'tuple', [
                    (f'value_{index}', canonical_type(ty))
                    for index, ty in enumerate(split(raw.rsplit(')', 1)[0]))
                ]))
            else:
                rows.append((value.split('=', 1)[0].strip(), 'unit', []))
        result[name] = kind, rows
    return result


def original_ast_items(path):
    aliases = original_aliases(path.with_name('semantic_scopes.rs'))
    names = {}
    tuples = {}
    generic = set()
    for alias, target in aliases.items():
        target = canonical_type(target)
        if tuple_parts(target) is not None:
            tuples[alias] = target
            continue
        match = re.fullmatch(r'(Core\w+)(<SemanticTypeRecipe>)?', target)
        if match is None:
            raise ValueError('unrecognized original AST alias: ' + alias + ' = ' + target)
        name = match.group(1)
        if name in names:
            raise ValueError('duplicate Core specialization: ' + name)
        names[name] = alias
        if match.group(2):
            generic.add(name)
    if not names:
        raise ValueError('missing public SemanticOriginal Core aliases')
    ORIGINAL_ALIASES.clear()
    ORIGINAL_ALIASES.update(tuples)
    core = declarations(path, names.keys())
    missing = names.keys() - core.keys()
    if missing:
        raise ValueError('missing shared Core AST declarations: ' + ', '.join(sorted(missing)))

    def specialize(ty):
        ty = canonical_type(ty)
        for old, new in names.items():
            if old in generic:
                ty = re.sub(r'\b' + old + r'<Annotation>', new, ty)
            else:
                ty = re.sub(r'\b' + old + r'\b', new, ty)
        ty = re.sub(r'\bAnnotation\b', 'SemanticTypeRecipe', ty)
        if re.search(r'\bCore\w+\b', ty):
            raise ValueError('Core AST reference must have a complete public alias: ' + ty)
        return canonical_type(ty)

    result = {}
    for name, alias in names.items():
        kind, rows = core[name]
        if kind == 'struct':
            rows = [(field, specialize(ty)) for field, ty in rows]
        else:
            rows = [(variant, shape, [(field, specialize(ty)) for field, ty in fs])
                    for variant, shape, fs in rows]
        result[alias] = kind, rows
    return result


def type_leaves(ty):
    ty = canonical_type(ty)
    if ty.startswith(('Box<', 'Vec<', 'Option<')):
        yield from type_leaves(ty[ty.index('<') + 1:-1])
    elif (parts := tuple_parts(ty)) is not None:
        for part in parts:
            yield from type_leaves(part)
    else:
        yield ty
