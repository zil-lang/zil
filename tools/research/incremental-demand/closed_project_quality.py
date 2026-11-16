#!/usr/bin/env python3
"""Quality for closed-project-runtime-v1; never substitutes for an old ELF gate.

Closed executables explicitly waive generated zil.fn.* exports. Current source,
independent Verify, full O2, physical CGU16/jobs1 ownership and all 256 byte+EOF
oracles remain mandatory. Complete ELF equality is an accurate observation, not
this protocol's decision rule. Missing generated definitions are enumerated;
foreign/entry exports and cross-unit references must survive. Formal admission
is closed+off only. Scalar C integer signatures, physical symbol binding and
relocation envelopes are checked; globals/callbacks/indirect calls are refused.
Unwind section bounds are checked, not general unwind/cleanup semantics. No
universal ABI, callback, exception or instruction-equivalence claim is made.
Sustained runtime acceptance is separate from functional quality: six balanced
samples, one warmup, B/A<=1.10, R/B>=0.90 per case, geomean R/B>=0.95.
These are existing finite-profile/product gates, not universal performance proof.

complete_imports is an explicit, default-false policy. Its separately named
closed-import-project-runtime-v1 profile validates complete transitive layout
imports against the current diagnostic call graph. Imports grant optimization
visibility, not canonical ownership or new exported definitions. Existing
closed-only results and negatives retain their original schema and policy.
"""
import copy
import json
import math
import re
import statistics
import struct

import native_link_matched_measure as matched
import elf_instruction_quality as late

SCHEMA = 'closed-project-runtime-v1'
IMPORT_SCHEMA = 'closed-import-project-runtime-v1'
INPUTS = (b'', *(bytes([i]) for i in range(256)))
RUNTIME_GATES = {'candidate_over_main_max_per_case': 1.10,
                 'rust_over_candidate_min_per_case': 0.90,
                 'rust_over_candidate_min_geomean': 0.95,
                 'warmups_per_arm': 1, 'samples_per_arm': 6,
                 'iterations': 10000, 'seed_period': 251,
                 'accumulator_modulus': 1000003}


def require(value, message):
    if not value:
        raise ValueError(message)


def policy(args):
    require(args.candidate_policy == 'closed' and args.function_merging == 'off', 'formal quality requires closed+off')
    require(type(getattr(args, 'complete_imports', False)) is bool, 'complete_imports must be an explicit boolean')


def schema(args):
    return IMPORT_SCHEMA if getattr(args, 'complete_imports', False) else SCHEMA


def policy_identity(args):
    return 'closed+off+complete-imports' if getattr(args, 'complete_imports', False) else 'closed+off'


def layout_roles(units, items, model, complete_imports):
    """Bind canonical ownership and, when present, the entire current closure."""
    require(len(units) == 16 and len({u['owner'] for u in units}) == 16
            and all(0 <= u['owner'] < 16 for u in units), 'sixteen unique physical layout owners')
    definitions = {i for i, (kind, _) in enumerate(items) if kind == 'define'}
    owned = [i for u in units for i in u['definitions']]
    require(sorted(owned) == sorted(definitions), 'each current definition needs exactly one canonical owner')
    for u in units:
        groups = [u[g] for g in ('prelude', 'declarations', 'imports', 'definitions')]
        require(all(len(g) == len(set(g)) and all(type(i) is int and 0 <= i < len(items) for i in g) for g in groups), 'duplicate/out-of-range layout indices')
        require(not set(u['imports']) & set(u['definitions']) and set(u['imports']) <= definitions,
                'imports must be nonowned current definitions')
        require(complete_imports or not u['imports'], 'nonempty imports require explicit complete_imports')
    expected = [{items[i][1] for i in u['definitions']} for u in units]
    owners = {name: index for index, names in enumerate(expected) for name in names}
    imported = [{items[i][1] for i in u['imports']} for u in units]
    has_imports = any(imported)
    if has_imports:
        by_name = {name: i for i, (_, name) in enumerate(items)}
        for index, unit in enumerate(units):
            pending = [ref for name in expected[index] for ref in model['calls'].get(name, set())]
            closure = set()
            while pending:
                name = pending.pop()
                if name not in owners or owners[name] == index or name in closure:
                    continue
                require(re.fullmatch(r'zil\.fn\.\d+', name) or name in model.get('classes', ()),
                        'imports need a current scalar source or full-body validated class definition')
                closure.add(name)
                pending.extend(model['calls'].get(name, set()))
            require(imported[index] == closure, 'imports must contain the complete current transitive call closure')
            references = {ref for name in expected[index] | closure for ref in model['calls'].get(name, set())}
            prelude = {by_name[n] for n in references if n in by_name and items[by_name[n]][0] == 'declare'}
            declarations = {by_name[n] for n in references if n in owners and owners[n] != index and n not in closure}
            require(set(unit['prelude']) == prelude and set(unit['declarations']) == declarations,
                    'current complete import prelude/declarations must match referenced roles')
    cross = {ref for caller, refs in model['calls'].items() for ref in refs
             if caller in owners and ref in owners and owners[caller] != owners[ref]
             and ref not in imported[owners[caller]]}
    return expected, cross, has_imports


def import_names(units, items):
    return [{f'zil.import.{u["owner"]}.{items[i][1]}' for i in u['imports']} for u in units]


def hit_preflight(receipt):
    access = receipt.get('cache') or {}
    require(receipt.get('input_kind') == 'object-cache'
            and receipt.get('llvm_verification_this_request') is False
            and receipt.get('producer_verified_before_after') is True
            and type(receipt.get('current_machine_preflight_reused')) is bool,
            'cache hit lacks typed current machine-preflight/producer verification scope')
    for value in (receipt.get('preflight_ns'), access.get('current_preflight_ns')):
        require(type(value) is int and value >= 0, 'cache hit lacks current preflight duration')
    require(receipt['preflight_ns'] == access['current_preflight_ns']
            and all(type(receipt.get(k)) is int and receipt[k] == 0
                    for k in ('builder_ns', 'parse_ns', 'optimize_ns', 'emit_ns')),
            'cache hit contradicts current preflight/native work scope')
    return receipt['current_machine_preflight_reused']


def producer_receipt(args, item):
    archived = json.loads(matched.recheck_archived(args.out, item['producer_native_receipt']))
    require(archived.get('object_sha256') == item['object']['sha256'] and archived.get('policy') == policy_identity(args), 'producer archive object/policy identity')
    value = archived['receipt']
    require(value.get('backend') == 'llvm-api' and value.get('pipeline') == 'default<O2>'
            and value.get('triple') == 'x86_64-unknown-linux-gnu'
            and value.get('llvm_version') == '23.1.4'
            and value.get('verify_before_after') is True
            and value.get('input_kind') == 'builder'
            and value.get('object_bytes') == item['object']['bytes']
            and (value.get('cache') or {}).get('state') == 'miss'
            and value['cache'].get('action_key') == item['action_key']
            and value['cache'].get('corrupt') is False,
            'archived producer no longer binds this full-O2 verified object')
    return value


def scalar_model(llvm):
    text = llvm.decode('ascii')
    require(not re.search(r'(?m)^[@%!]|\b(?:blockaddress|indirectbr|callbr|invoke|asm|alias|ifunc|personality|global|constant)\b', text),
            'globals/address/callback/exception LLVM outside scalar ABI scope')
    headers = list(re.finditer(r'(?m)^(declare|define) (.+?) @([A-Za-z0-9_.]+)\(([^\n]*)\)([^\n]*)$', text))
    require(headers and sum(line.startswith(('declare ', 'define ')) for line in text.splitlines()) == len(headers), 'unknown scalar function envelope')
    signatures, calls, definitions, bodies = {}, {}, set(), {}
    classes = set()
    intrinsics = {'llvm.trap': ('void', ''),
                  **{f'llvm.{op}.with.overflow.i32': ('{ i32, i1 }', 'i32, i32') for op in ('sadd', 'ssub', 'smul')}}
    for h in headers:
        kind, result, name, params, suffix = h.groups()
        require(name not in signatures, 'duplicate scalar ABI name')
        if name in intrinsics:
            require(kind == 'declare' and (result, params) == intrinsics[name] and not suffix, 'unknown scalar intrinsic ABI')
        else:
            require(result in ('i32', 'void') and all(re.fullmatch(r'i32(?: %[A-Za-z0-9_.]+)?', p) for p in params.split(', ') if p)
                    and suffix in (' {', ' nonlazybind', ''), 'unknown scalar C integer ABI')
            require((kind == 'declare' and name in ('getchar', 'putchar')) or
                    (kind == 'define' and (name == 'main' or re.fullmatch(r'zil\.fn\.\d+', name)
                                           or re.fullmatch(r'zil\.class\.[0-9a-f]{64}', name))),
                    'unknown scalar foreign/definition role')
            if name.startswith('zil.class.'):
                classes.add(name)
        signatures[name] = (result, params)
        if kind == 'define':
            definitions.add(name)
            end = text.find('\n}', h.end())
            require(end >= 0, 'incomplete scalar definition')
            body = text[h.end():end]
            bodies[name] = (result, params, suffix, body)
            refs = re.findall(r'@([A-Za-z0-9_.]+)', body)
            direct = re.findall(r'\bcall [^\n@]*@([A-Za-z0-9_.]+)\(', body)
            require(refs == direct and len(re.findall(r'\bcall\b', body)) == len(direct), 'address-taken or indirect scalar call refused')
            calls[name] = set(direct)
    require(signatures.get('main') == ('i32', '') and signatures.get('getchar') == ('i32', '')
            and signatures.get('putchar') == ('i32', 'i32'), 'exact C main/getchar/putchar ABI required')
    require(all(ref in signatures for refs in calls.values() for ref in refs), 'undeclared scalar call')
    # The spelling only locates candidates. Every class must be an exact clone
    # of a current source definition, including signature, SSA, blocks and calls.
    source_bodies = {}
    for name, body in bodies.items():
        if re.fullmatch(r'zil\.fn\.\d+', name):
            source_bodies.setdefault(body, []).append(name)
    class_sources = {}
    parents = {name: [] for name in classes}
    remaining = {}
    for name in classes:
        require(bodies[name] in source_bodies, 'class lacks an equal complete current source body')
        class_sources[name] = sorted(source_bodies[bodies[name]])
        require(calls[name] <= classes | intrinsics.keys(), 'class must contain only pure class/intrinsic direct calls')
        require(signatures[name][0] == 'i32', 'class requires the finite i32 result ABI')
        allowed_ops = {'add', 'sub', 'mul', 'udiv', 'sdiv', 'urem', 'srem',
                       'shl', 'lshr', 'ashr', 'and', 'or', 'xor', 'icmp',
                       'trunc', 'zext', 'sext', 'extractvalue', 'call', 'phi',
                       'br', 'ret', 'unreachable', 'select'}
        body = bodies[name][3]
        require(not re.search(r'\b(?:ptr|undef|poison|zeroinitializer)\b', body)
                and all(bits in ('1', '32') for bits in re.findall(r'\bi(\d+)\b', body)),
                'class pointer/undefined/nonprofile scalar value')
        for line in body.splitlines():
            line = line.strip()
            if not line or re.fullmatch(r'[A-Za-z0-9_.]+:', line):
                continue
            instruction = re.fullmatch(r'(?:%[A-Za-z0-9_.]+ = )?([a-z]+)(?: (.*))?', line)
            require(instruction and instruction[1] in allowed_ops, 'unknown or impure class operation')
            if instruction[1] == 'call':
                call = re.fullmatch(r'(.+?) @([A-Za-z0-9_.]+)\((.*)\)', instruction[2])
                require(call and call[2] in signatures, 'class direct call envelope')
                result, params = signatures[call[2]]
                actual = call[3].split(', ') if call[3] else []
                expected = params.split(', ') if params else []
                require(call[1] == result and len(actual) == len(expected)
                        and all(re.fullmatch(r'i32 (?:%[A-Za-z0-9_.]+|-?\d+)', arg)
                                and param.split(' ')[0] == 'i32'
                                for arg, param in zip(actual, expected)),
                        'class direct call must match complete current scalar ABI')
        dependencies = calls[name] & classes
        remaining[name] = len(dependencies)
        for dependency in dependencies:
            parents[dependency].append(name)
    ready = [name for name in classes if remaining[name] == 0]
    visited = 0
    while ready:
        visited += 1
        for parent in parents[ready.pop()]:
            remaining[parent] -= 1
            if remaining[parent] == 0:
                ready.append(parent)
    require(visited == len(classes), 'class implementations must form an acyclic pure call graph')
    return {'definitions': definitions, 'calls': calls, 'foreign': {'getchar', 'putchar'},
            'classes': classes, 'class_sources': class_sources}


def elf_table(data, executable=False):
    """Bounded ELF64/x86-64 symbol and relocation reader, no execution/tools."""
    require(len(data) >= 64 and len(data) <= 128*1024*1024 and data[:7] == b'\x7fELF\x02\x01\x01', 'bounded little-endian ELF64 required')
    head = struct.unpack_from('<16sHHIQQQIHHHHHH', data)
    require(head[1] in ((2, 3) if executable else (1,)) and head[2] == 62 and head[3] == 1
            and head[8] == 64 and head[11] == 64 and 0 < head[12] < 65535
            and head[6]+64*head[12] <= len(data), 'unsupported ELF target/header/section envelope')
    sections = [struct.unpack_from('<IIQQQQIIQQ', data, head[6]+64*i) for i in range(head[12])]
    for s in sections:
        require(s[1] == 8 or s[4]+s[5] <= len(data), 'ELF section/unwind envelope outside file')
    def name_at(table, offset):
        require(offset < len(table) and table.find(b'\0', offset) >= 0, 'ELF symbol string bounds')
        return table[offset:table.index(b'\0', offset)].decode('ascii').split('@')[0]
    tables, symbols = {}, []
    for i, s in enumerate(sections):
        if s[1] not in (2, 11):
            continue
        require(s[9] == 24 and s[5] % 24 == 0 and s[6] < len(sections), 'ELF symbol table bounds')
        strings = sections[s[6]]
        require(strings[1] == 3, 'ELF symbol string table type')
        string_data = data[strings[4]:strings[4]+strings[5]]
        table = []
        for pos in range(s[4], s[4]+s[5], 24):
            n, info, other, section, address, size = struct.unpack_from('<IBBHQQ', data, pos)
            entry = dict(name=name_at(string_data, n), kind=info & 15, bind=info >> 4,
                         visibility=other & 3, section=section, address=address, size=size)
            if section and section < 0xff00:
                require(section < len(sections), 'ELF symbol section bounds')
                sec = sections[section]
                offset = address-(sec[3] if executable else 0)
                require(0 <= offset <= sec[5] and offset+size <= sec[5], 'ELF symbol extent bounds')
                if entry['kind'] == 2:
                    require(sec[2] & 4, 'physical function must occupy executable section')
            table.append(entry)
            symbols.append(entry)
        tables[i] = table
    require(tables, 'physical ELF symbol tables required')
    relocations = []
    for s in sections:
        require(s[1] != 9, 'ELF REL without addends outside scalar scope')
        if s[1] != 4:
            continue
        require(s[9] == 24 and s[5] % 24 == 0 and s[6] in tables and s[7] < len(sections), 'ELF relocation table bounds')
        for pos in range(s[4], s[4]+s[5], 24):
            offset, info, addend = struct.unpack_from('<QQq', data, pos)
            require(info >> 32 < len(tables[s[6]]), 'ELF relocation symbol bounds')
            target = tables[s[6]][info >> 32]
            kind = info & 0xffffffff
            if not executable:
                require(kind in (2, 4, 9, 41, 42) and offset+4 <= sections[s[7]][5], 'unknown scalar object relocation')
            else:
                require(kind in (6, 7, 8) and any(sec[2] & 2 and sec[3] <= offset and offset+8 <= sec[3]+sec[5] for sec in sections), 'unknown dynamic relocation/target extent')
            relocations.append(dict(symbol=target, kind=kind, offset=offset, addend=addend))
    return {'header': head, 'sections': sections, 'symbols': symbols, 'relocations': relocations}


def defined_functions(table):
    result = {}
    for s in table['symbols']:
        if s['name'] and s['kind'] == 2 and s['section']:
            require(s['name'] not in result or result[s['name']] == s, 'ambiguous physical function binding')
            result[s['name']] = s
    return result


def object_abi(data, expected, cross, model, private_imports=()):
    table = elf_table(data)
    funcs = defined_functions(table)
    require(set(funcs) <= expected | set(private_imports), 'object defines foreign/unknown scalar function')
    require(all(funcs[n]['bind'] == 0 and funcs[n]['visibility'] == 0 and funcs[n]['size'] > 0
                for n in set(funcs) & set(private_imports)), 'surviving private import must be LOCAL/default with exact owner-derived name')
    require(not any(s['section'] and s['kind'] in (1, 6, 10) for s in table['symbols']), 'object globals/TLS/IFUNC outside scalar scope')
    protected = cross | ({'main'} & expected)
    require(protected <= funcs.keys() and all(funcs[n]['bind'] == 1 and funcs[n]['visibility'] == 0 and funcs[n]['size'] > 0 for n in protected), 'protected entry/cross-unit symbol lost global default function binding')
    undefined = {s['name'] for s in table['symbols'] if s['name'] and not s['section']}
    require(undefined <= (model['definitions'] | model['foreign'])-expected, 'unknown scalar object foreign import')
    require(all(s['bind'] == 1 and s['visibility'] == 0 and s['kind'] in (0, 2) for s in table['symbols'] if s['name'] in undefined), 'scalar foreign/cross import binding')
    reloc_names = {r['symbol']['name'] for r in table['relocations']}
    require(undefined <= reloc_names, 'undefined scalar import lacks actual relocation')
    for r in table['relocations']:
        s = r['symbol']
        section = table['sections'][s['section']] if s['section'] < len(table['sections']) else None
        constant_pool = (s['kind'] == 0 and s['bind'] == 0 and s['visibility'] == 0
                         and re.fullmatch(r'\.LCPI\d+_\d+', s['name']) is not None
                         and section is not None and section[1] == 1 and section[2] == 18
                         and section[9] > 0 and s['address'] % section[9] == 0
                         and s['address']+section[9] <= section[5]
                         and r['kind'] == 2 and r['addend'] == -4)
        require((s['name'] in funcs or s['name'] in undefined) or
                (s['kind'] == 3 and section is not None and section[2] & 4) or constant_pool,
                'unknown address/data relocation outside scalar call/unwind envelope')
    return {'functions': funcs, 'undefined': sorted(undefined), 'relocated_imports': sorted(undefined & reloc_names)}


def executable_abi(args, row):
    inv = row['inventory']
    llvm = matched.recheck_archived(args.out, row['llvm'])
    model = scalar_model(llvm)
    image = elf_table(matched.recheck_archived(args.out, row['executable']), executable=True)
    funcs = defined_functions(image)
    require(all(n in funcs and funcs[n]['bind'] == 1 and funcs[n]['visibility'] == 0 and funcs[n]['size'] > 0 for n in ('main', '_start'))
            and image['header'][4] == funcs['_start']['address'], 'actual ELF C main/start entry binding')
    items = [(m[1].decode(), m[2].decode()) for m in re.finditer(rb'(?m)^(declare|define) [^\n]*?@([^ (]+)\(', llvm)]
    private = [set() for _ in range(16)]
    for layout in inv.get('partition_layouts', []):
        if layout.get('key') in inv['current_layout_matches'] or layout.get('sha256') in inv['current_layout_matches']:
            units = matched.layout_units(matched.recheck_archived(args.out, layout))
            layout_roles(units, items, model, getattr(args, 'complete_imports', False))
            private = import_names(units, items)
            break
    seen, imports, owners = {}, set(), {}
    for obj in inv['objects']:
        data = matched.recheck_archived(args.out, obj['object'])
        table = elf_table(data)
        physical = defined_functions(table)
        require(set(physical) == set(obj['defined_functions']) and not (set(physical) & seen.keys()), 'actual unique CGU function ownership')
        for name in physical:
            if name.startswith('zil.import.'):
                require(getattr(args, 'complete_imports', False) and name in private[obj['unit_index']]
                        and physical[name]['bind'] == 0 and physical[name]['visibility'] == 0,
                        'ELF private import clone needs exact current layout owner/name and LOCAL/default binding')
        for name, original in physical.items():
            require(name in funcs and all(funcs[name][k] == original[k] for k in ('kind', 'bind', 'visibility', 'size')),
                    'final ELF lost/rebound a physical CGU function')
            seen[name] = original
            owners[name] = obj['unit_index']
        imports.update(s['name'] for s in table['symbols'] if s['name'] and not s['section'])
    require({n for n in funcs if n == 'main' or n.startswith('zil.')} == set(seen), 'final ELF generated definitions differ from CGU union')
    cross = set(inv['required_cross_unit_exports']) if 'required_cross_unit_exports' in inv else {
        ref for caller, refs in model['calls'].items() for ref in refs
        if caller in owners and ref in owners and owners[caller] != owners[ref]}
    cross |= imports & model['definitions']
    require(cross <= seen.keys() and all(funcs[n]['bind'] == 1 and funcs[n]['visibility'] == 0 for n in cross), 'final ELF cross-unit binding')
    crt = {'__libc_start_main', '__cxa_finalize', '_ITM_deregisterTMCloneTable', '_ITM_registerTMCloneTable', '__gmon_start__'}
    undefined = {s['name'] for s in image['symbols'] if s['name'] and not s['section']}
    require(undefined == model['foreign'] | crt and model['foreign'] <= imports, 'final ELF foreign import set')
    require(all(s['kind'] == 2 and s['bind'] == 1 and s['visibility'] == 0 for s in image['symbols'] if s['name'] in model['foreign']), 'final ELF foreign function ABI binding')
    require(model['foreign'] <= {r['symbol']['name'] for r in image['relocations'] if r['kind'] in (6, 7)}, 'final foreign imports need actual dynamic relocations')
    return {'scope': 'scalar C main/i32 foreign/protected cross-unit bindings; relocation/unwind envelopes only; callbacks/globals/cleanup ABI not claimed',
            'entry': funcs['_start']['address'], 'foreign_imports': sorted(model['foreign']), 'cross_unit_exports': sorted(cross)}


def source_metadata(files):
    hashes = {name: matched.rc.sha(text.encode()) for name, text in sorted(files.items())}
    return {'file_hashes': hashes, 'source_files': len(files),
            'source_bytes': sum(len(text.encode()) for text in files.values()),
            'identity_sha256': matched.rc.sha(json.dumps(hashes, sort_keys=True).encode())}


def current_verify(report):
    return (report.get('status') == 'Success' and report.get('diagnostics') == []
            and report.get('verification_runs') == 1
            and sum(e == {'phase': 'verify', 'disposition': 'executed'}
                    for e in report.get('query_events', [])) == 1)


def resource_limit(row):
    report = row.get('resident', {}).get('report', {})
    return (row.get('arm') == 'baseline' and report.get('status') == 'ResourceLimit'
            and report.get('phase') == 'expand' and report.get('verification_runs') == 0
            and any(d.get('category') == 'ResourceLimit' for d in report.get('diagnostics', []))
            and not row.get('native') and not row.get('executable')
            and isinstance(row.get('publication_before'), dict) and isinstance(row.get('publication_after'), dict)
            and row.get('publication_before') == row.get('publication_after'))


def checked_oracles(row, oracle, repetitions):
    outcomes = row.get('native', [])
    domain = (b'A',) if getattr(oracle, 'sustained', False) else INPUTS
    expected_inputs = [data.hex() for _ in range(repetitions) for data in domain]
    require([o.get('stdin_hex') for o in outcomes] == expected_inputs,
            'complete ordered 256-byte plus EOF domain required')
    for result in outcomes:
        expected = oracle.oracle('scalar', bytes.fromhex(result['stdin_hex']), '')
        require(result.get('valid') is True and result.get('actual') == result.get('expected') == expected,
                'independent full-domain numerical oracle failed')


def inventory(args, report, cache, llvm, executable_sha256, request_directory=None):
    """Compose physical inventory primitives, with an explicit closed export rule.

    Ordinary main uses the frozen exact-ownership implementation. Closed units
    can lose only local generated names. Layout identity, full object hashes,
    actual cache observations and current O2 receipts are still checked here.
    """
    policy(args)
    candidate = str(cache.parent.name) == 'candidate'
    if not candidate:
        return matched.zil_inventory(args, report, cache, llvm, executable_sha256, request_directory)
    cg = report.get('native_codegen') or {}
    require(cg.get('backend') == 'llvm-api' and cg.get('requested_units') == cg.get('actual_units') == 16
            and cg.get('jobs') == 1 and cg.get('fallback') is None,
            'actual closed backend/CGU16/jobs1 contract')
    artifact = matched.final_artifact_scope(args, report)
    if artifact is not None and artifact['before_codegen']:
        # The frozen helper rehashes every archived physical producer object,
        # envelope, symbol receipt and layout. This does not fabricate fresh
        # per-unit native work; current full preflight is checked by scope().
        result = matched.retained_inventory(args, report, cache, llvm, executable_sha256, artifact)
        for item in result['objects']:
            producer_receipt(args, item)
        cache_summary = cg.get('object_cache') or {}
        require(all(type(cache_summary.get(k)) is int and cache_summary[k] == 0 for k in
                    ('current_preflight_ns', 'object_hit_preflight_checked_units', 'object_hit_preflight_reused_units')),
                'retained artifact cannot fabricate current object-cache preflights')
        return result
    model = scalar_model(llvm)
    reports = [json.loads(r) if isinstance(r, str) else r for r in cg.get('object_reports', [])]
    require(len(reports) == 16, 'sixteen current closed unit receipts')
    schema = cache/'objects-v1'
    namespaces = list(schema.iterdir()) if schema.is_dir() and not schema.is_symlink() else []
    require(len(namespaces) == 1 and namespaces[0].is_dir() and not namespaces[0].is_symlink()
            and re.fullmatch('[0-9a-f]{64}', namespaces[0].name), 'unique actual object namespace')
    namespace = namespaces[0]
    bindings = getattr(args, '_closed_namespaces', {})
    key = str(cache.resolve())
    require(key not in bindings or bindings[key] == namespace.name, 'closed namespace changed')
    bindings[key] = namespace.name
    args._closed_namespaces = bindings
    objects = []
    for index, receipt in enumerate(reports):
        require(receipt.get('backend') == 'llvm-api' and receipt.get('pipeline') == 'default<O2>',
                'full default O2 unit pipeline required')
        access = receipt.get('cache') or {}
        action = access.get('action_key', '')
        require(re.fullmatch('[0-9a-f]{64}', action) is not None, 'exact object action key')
        path = namespace/(action+'.zobj')
        require(not path.is_symlink() and path.is_file() and path.resolve().parent == namespace.resolve(), 'physical closed object')
        data = path.read_bytes()
        require(len(data) > 80 and data[:8] == b'ZILOBJ01' and data[8:40].hex() == action
                and data[40:72].hex() == matched.rc.sha(data[80:])
                and int.from_bytes(data[72:80], 'little') == len(data)-80 == receipt.get('object_bytes'),
                'complete object envelope/key/hash/size')
        item = matched.object_record(args, path, cache, data[80:])
        producers = getattr(args, '_closed_object_producers', {})
        producer_key = (str(cache.resolve()), action, item['object']['sha256'], policy_identity(args))
        if access.get('state') == 'miss':
            require(receipt.get('verify_before_after') is True, 'fresh closed object needs actual before/after LLVM Verify')
            archived = {'receipt': receipt, 'object_sha256': item['object']['sha256'], 'policy': policy_identity(args)}
            producers[producer_key] = matched.blob(args.out, json.dumps(archived, sort_keys=True).encode(), '.producer.json')
        else:
            require(access.get('state') == 'hit' and producer_key in producers, 'closed cache hit needs same-arm full-O2 verified producer')
            hit_preflight(receipt)
        args._closed_object_producers = producers
        item.update(unit_index=index, action_key=action, cache_observation=access,
                    cache_container=matched.preserve(args.out, path, '.zobj'),
                    producer_native_receipt=producers[producer_key],
                    defined_functions=sorted(matched.function_symbols(data[80:])))
        producer_receipt(args, item)
        objects.append(item)
    items = [(m[1].decode(), m[2].decode()) for m in re.finditer(rb'(?m)^(declare|define) [^\n]*?@([^ (]+)\(', llvm)]
    definitions = {i for i, (kind, _) in enumerate(items) if kind == 'define'}
    layouts = []
    for path in sorted((cache/'partition-layout-v1').glob('*.layout')):
        require(not path.is_symlink(), 'layout symlink')
        data = path.read_bytes()
        require(len(data) >= 76 and data[:8] == b'ZILCGU01' and data[8:40].hex() == path.stem
                and matched.rc.sha(data[72:]) == data[40:72].hex(), 'complete physical layout binding')
        units = matched.layout_units(data)
        if len(units) != 16:
            continue
        indices = [i for u in units for group in ('prelude', 'declarations', 'imports', 'definitions') for i in u[group]]
        owned = [i for u in units for i in u['definitions']]
        if (not items or any(i >= len(items) for i in indices) or sorted(owned) != sorted(definitions)
                or (not getattr(args, 'complete_imports', False) and any(u['imports'] for u in units))):
            continue
        try:
            expected, cross, has_imports = layout_roles(units, items, model, getattr(args, 'complete_imports', False))
        except ValueError:
            # A persistent cache retains prior request layouts. A valid envelope
            # can describe an older call graph; it is not current provenance.
            # Only complete current closures matching physical objects survive
            # below, and at least one unique current layout remains mandatory.
            continue
        private = import_names(units, items)
        actual = [set(o['defined_functions']) for o in objects]
        if not all(a <= e | p for a, e, p in zip(actual, expected, private)):
            continue
        owners = {name: i for i, names in enumerate(expected) for name in names}
        canonical_actual = [a-p for a, p in zip(actual, private)]
        removed = set.union(*expected)-set.union(*canonical_actual)
        allowed = {name for name in owners if re.fullmatch(r'zil\.fn\.\d+(?:\.lc1)?', name)
                   or name in model.get('classes', ())}-cross
        if not removed <= allowed:
            continue
        for item, names, clones in zip(objects, expected, private):
            item['scalar_abi'] = object_abi(matched.recheck_archived(args.out, item['object']), names, cross & names, model, clones)
        undefined = {n for o in objects for n in o['scalar_abi']['undefined']}
        if not has_imports:
            require(cross <= undefined, 'required cross-unit calls lack physical object imports')
        cross |= undefined & model['definitions']
        physical = {n: info for o in objects for n, info in o['scalar_abi']['functions'].items()}
        require(cross <= physical.keys() and all(physical[n]['bind'] == 1 and physical[n]['visibility'] == 0 for n in cross),
                'actual canonical cross-unit imports need physical GLOBAL/default targets')
        layouts.append({'units': units, 'key': path.stem, 'payload_sha256': matched.rc.sha(data[72:]),
                        'removed_functions': sorted(removed),
                        'required_cross_unit_exports': sorted(cross),
                        'complete_imports': getattr(args, 'complete_imports', False),
                        'actual_import_count': sum(len(u['imports']) for u in units),
                        'private_import_functions': sorted(set.union(*actual) & set.union(*private)),
                        **matched.preserve(args.out, path, '.layout')})
    require(layouts and len({matched.rc.sha(json.dumps(l['units'], sort_keys=True).encode()) for l in layouts}) == 1,
            'closed objects must uniquely match full current layout and protected export ownership')
    states = {state: sum(o['cache_observation'].get('state') == state for o in objects) for state in ('hit', 'miss')}
    aggregate = cg.get('object_cache') or {}
    require(sum(states.values()) == 16 and aggregate.get('hits') == states['hit']
            and aggregate.get('misses') == states['miss'] and aggregate.get('corrupt') == 0
            and aggregate.get('write_failures') == 0, 'actual closed cache accounting')
    checked = sum(r['cache']['state'] == 'hit' and not r['current_machine_preflight_reused'] for r in reports)
    reused = sum(r['cache']['state'] == 'hit' and r['current_machine_preflight_reused'] for r in reports)
    require(all(type(aggregate.get(k)) is int for k in
                ('object_hit_preflight_checked_units', 'object_hit_preflight_reused_units', 'current_preflight_ns'))
            and aggregate['object_hit_preflight_checked_units'] == checked
            and aggregate['object_hit_preflight_reused_units'] == reused
            and checked+reused == states['hit']
            and aggregate['current_preflight_ns'] == sum(r['cache']['current_preflight_ns'] for r in reports),
            'aggregate current machine-preflight counts/durations disagree with actual hits')
    result = {'object_count': 16, 'objects': objects, 'partition_layouts': layouts,
              'current_layout_matches': [l['sha256'] for l in layouts], 'compiler_summary': cg,
              'current_object_report_count': 16, 'removed_functions': layouts[0]['removed_functions'],
              'required_cross_unit_exports': layouts[0]['required_cross_unit_exports'],
              'policy_identity': policy_identity(args),
              'actual_import_count': layouts[0]['actual_import_count'],
              'private_import_functions': layouts[0]['private_import_functions'],
              'provenance_kind': 'current-closed-object-reports'}
    histories = getattr(args, '_zil_producer_inventories', {})
    histories[(str(cache.resolve()), matched.pc.llvm_comparison_sha256(llvm), executable_sha256)] = copy.deepcopy(result)
    args._zil_producer_inventories = histories
    return result


def quality(rows, args, oracle, allow_main_limit=False):
    policy(args)
    require(len(rows) == 3 and {r['arm'] for r in rows} == set(matched.ARMS), 'complete actual three-arm cell')
    arms = {r['arm']: r for r in rows}
    baseline_limit = allow_main_limit and resource_limit(arms['baseline'])
    active = [r for r in rows if not (baseline_limit and r['arm'] == 'baseline')]
    require(all(r.get('valid') is True for r in active), 'failed request is not a comparison')
    zil = [r for r in active if r['arm'] != 'rust']
    reports = [r['resident']['report'] for r in zil]
    fields = ('status', 'phase', 'diagnostics', 'input_fingerprint', 'source_files', 'scalar_interfaces')
    require(all(all(k in r for k in fields) for r in reports), 'complete current semantic report fields')
    require(all(all(r.get(k) == reports[0].get(k) for k in fields) for r in reports), 'current source/diagnostic/interface identity')
    error = rows[0]['request'] == 'error'
    if not error:
        require(all(current_verify(r) for r in reports), 'one current independent Verify per successful request')
    elf_equal = aggregate_equal = None
    exports = {}
    if args.mode == 'o2' and not error:
        for row in active:
            checked_oracles(row, oracle, args.native_repetitions)
            data = matched.recheck_archived(args.out, row['executable'])
            require(data.startswith(b'\x7fELF'), 'complete ELF publication required')
            if row['arm'] != 'rust':
                inv = row['inventory']
                require(inv['object_count'] == len(inv['objects']) == inv['compiler_summary']['actual_units'] == 16
                        and inv['current_layout_matches'], 'physical sixteen-unit quality required')
                exports[row['arm']] = sorted({n for o in inv['objects'] for n in o['defined_functions']}
                                             - set(inv.get('private_import_functions', [])))
                row['scalar_abi'] = executable_abi(args, row)
            else:
                require(row['inventory'].get('objects'), 'Rust physical incremental objects required')
        if not baseline_limit:
            elf_equal = (matched.recheck_archived(args.out, arms['baseline']['executable']) ==
                         matched.recheck_archived(args.out, arms['candidate']['executable']))
            aggregate_equal = exports['baseline'] == exports['candidate']
            missing = set(exports['baseline'])-set(exports['candidate'])
            extra = set(exports['candidate'])-set(exports['baseline'])
            inv = arms['candidate']['inventory']
            require(not extra and missing == set(inv.get('removed_functions', [])),
                    'aggregate export change must exactly equal declared physical closed removals')
    return {'schema': schema(args), 'policy_identity': policy_identity(args), 'valid': True, 'functional_quality_valid': True,
            'runtime_acceptance_pending': args.mode == 'o2', 'main_resource_limit': bool(baseline_limit),
            'full_domain_oracle': not getattr(oracle, 'sustained', False),
            'main_success': not baseline_limit, 'complete_ELF_equal': elf_equal,
            'aggregate_exports_equal_observed': aggregate_equal, 'aggregate_exports': exports,
            'candidate_removed_functions': arms['candidate'].get('inventory', {}).get('removed_functions', []),
            'private_import_functions': arms['candidate'].get('inventory', {}).get('private_import_functions', []),
            'full_ELF_equality_is_acceptance_prerequisite': False,
            'scope': 'source/current Verify/full-domain oracle/physical CGU and declared closed exports; sustained runtime gate is separate'}


def paired_comparison(rows, blocks, arms):
    samples = {}
    for arm in arms:
        selected = sorted((r for r in rows if r['arm'] == arm), key=lambda r: r['block'])
        require([r['block'] for r in selected] == list(range(blocks)) and all(r['valid'] for r in selected), 'complete valid paired blocks')
        values = [r['measurement']['wall_ms'] for r in selected]
        require(all(type(v) in (int, float) and math.isfinite(v) and v > 0 for v in values), 'positive finite complete-request samples')
        samples[arm] = values
    result = {'wall_ms_by_block': samples, 'median_ms': {a: statistics.median(v) for a, v in samples.items()}}
    for other in arms:
        if other != 'candidate':
            result['candidate_over_'+other+'_paired_GM'] = math.exp(statistics.mean(
                math.log(c/b) for c, b in zip(samples['candidate'], samples[other])))
    return result


def runtime_decision(cases, args=None):
    require(cases, 'nonempty sustained runtime profile')
    efficiencies, results = [], []
    complete_main = True
    for case in cases:
        measurements = case['samples']
        main_missing = case.get('main_resource_limit') is True
        complete_main &= not main_missing
        arms = ('candidate', 'rust') if main_missing else matched.ARMS
        require(set(measurements) == set(arms), 'exact sustained arm set; no fabricated main sample')
        for arm in arms:
            values = measurements[arm]
            require(len(values) == 6 and all(v.get('valid') is True and not v.get('timed_out')
                    and math.isfinite(v['wall_ms']) and v['wall_ms'] > 0 for v in values), 'six valid sustained samples per arm')
        medians = {a: statistics.median(v['wall_ms'] for v in measurements[a]) for a in arms}
        efficiency = medians['rust']/medians['candidate']
        regression = None if main_missing else medians['candidate']/medians['baseline']
        efficiencies.append(efficiency)
        results.append({'case': case['case'], 'main_resource_limit': main_missing,
                        'candidate_over_main': regression, 'rust_over_candidate': efficiency,
                        'median_ms': medians,
                        'wall_ms_by_sample': {a: [v['wall_ms'] for v in measurements[a]] for a in arms},
                        'p95_ms': {a: sorted(v['wall_ms'] for v in measurements[a])[-1] for a in arms},
                        'peak_rss_kib': {a: max(v['wait4_maxrss_kib'] for v in measurements[a]) for a in arms},
                        'rust_gate_pass': efficiency >= RUNTIME_GATES['rust_over_candidate_min_per_case'],
                        'main_gate_pass': None if main_missing else regression <= RUNTIME_GATES['candidate_over_main_max_per_case']})
    gm = math.exp(statistics.mean(math.log(v) for v in efficiencies))
    rust_pass = all(r['rust_gate_pass'] for r in results) and gm >= RUNTIME_GATES['rust_over_candidate_min_geomean']
    main_pass = (False if any(r['main_gate_pass'] is False for r in results)
                 else True if complete_main else None)
    return {'schema': schema(args), 'cases': results, 'rust_efficiency_geomean': gm,
            'candidate_rust_accepted': rust_pass,
            'main_regression_accepted': main_pass,
            'accepted': rust_pass and main_pass is True,
            'scope': 'bounded separate wrapper profile; p95 is nearest-rank with six samples, not a stable tail estimate; RSS observed, no invented memory cutoff'}
