"""Bind a research postlink hook to current witnessed input; no tool execution."""
import json
import os
from pathlib import Path
import re
from common import identity, sha, write
from visibility import parse, SYMBOL

ENVIRONMENT = ['ZIL_PRIVATE_IMPORTS_MANIFEST', 'ZIL_PRIVATE_IMPORTS_MANIFEST_SHA256',
               'ZIL_PRIVATE_IMPORTS_REQUEST_ID', 'ZIL_PRIVATE_IMPORTS_RECEIPT_DIR']


def prepare_hook(log, protocol, current, indexed, visibility):
    receipt_path = Path(os.environ['ZIL_RESEARCH_THINLTO_RECEIPT'])
    if json.loads(receipt_path.read_text()) != current:
        raise ValueError('current Rust receipt changed before hook manifest')
    witness = log / 'prelink-visibility.json'
    if json.loads(witness.read_text()) != visibility:
        raise ValueError('actual prelink witness changed before hook manifest')
    receipts = log / 'private-imports'
    receipts.mkdir(exist_ok=False)
    manifest = {'format': 'zil-thinlto-private-imports-manifest/1',
                'request_id': os.urandom(16).hex(),
                'protocol_sha256': os.environ['ZIL_THINLTO_PROTOCOL_SHA'],
                'plugin': protocol['tools']['plugin'],
                'rust_receipt': identity(receipt_path),
                'prelink_witness': identity(witness), 'units': []}
    for index in range(8):
        item = indexed[index]
        text = Path(item['prelink_ir']['path']).read_text()
        names = re.findall(r'^source_filename = "([^"\\]+)"$', text, re.M)
        if len(names) != 1 or not Path(names[0]).is_absolute():
            raise ValueError('one unescaped absolute source_filename required')
        manifest['units'].append({
            'index': index, 'source_filename': names[0],
            'input_bitcode': identity(item['original_output']),
            'canonical': current['visibility_proof']['units'][index]['canonical'],
            'prelink_definitions': sorted(parse(text)['definitions']),
            'required_prelink': visibility['units'][index]['required_prelink'],
            'allowed_original': visibility['units'][index]['allowed_original']})
    if len({v['source_filename'] for v in manifest['units']}) != 8:
        raise ValueError('nonunique backend source_filename')
    path = log / 'private-import-manifest.json'
    write(path, manifest)
    os.environ.update(dict(zip(ENVIRONMENT, [str(path), sha(path), manifest['request_id'], str(receipts)])))
    return path, manifest, receipts


def _normalize_header(text, selected):
    lines = []
    for line in text.splitlines(keepends=True):
        if line.startswith('define ') and any('@' + name + '(' in line for name in selected):
            name_at = line.index('@')
            # LLVM's local linkage setter also makes the symbol DSO-local.
            # Only these three linkage/DSO words may differ; attributes, return
            # types, call bodies, metadata and every other byte remain exact.
            prefix = re.sub(r'\b(?:available_externally|internal|dso_local)\s+', '', line[:name_at])
            line = prefix + line[name_at:]
        lines.append(line)
    return ''.join(lines)


def check_hooks(path, manifest, receipts, imported, protocol):
    if json.loads(path.read_text()) != manifest or sha(path) != os.environ['ZIL_PRIVATE_IMPORTS_MANIFEST_SHA256']:
        raise ValueError('hook manifest mutated')
    expected = {f'unit-{i}{suffix}' for i in range(8)
                for suffix in ['.started.json', '.before.ll', '.after.ll', '.json']}
    if {v.name for v in receipts.iterdir()} != expected:
        raise ValueError('missing/extra postlink hook output or backend execution')
    records = []
    for unit in manifest['units']:
        index = unit['index']
        record_path = receipts / f'unit-{index}.json'
        record = json.loads(record_path.read_text())
        if (record.get('format') != 'zil-thinlto-private-imports-receipt/1'
                or record.get('request_id') != manifest['request_id']
                or record.get('unit') != index or record.get('phase') != 'ThinLTOPostLink'
                or record.get('source_filename') != unit['source_filename']
                or record.get('manifest_sha256') != sha(path)
                or record.get('plugin_sha256') != protocol['tools']['plugin']['sha256']
                or record.get('rust_receipt_sha256') != manifest['rust_receipt']['sha256']
                or record.get('prelink_witness_sha256') != manifest['prelink_witness']['sha256']
                or record.get('verify_before') is not True or record.get('verify_after') is not True):
            raise ValueError('hook did not execute on current approved module')
        before_path, after_path = [receipts / f'unit-{index}.{kind}.ll' for kind in ['before', 'after']]
        if record.get('before') != identity(before_path) or record.get('after') != identity(after_path):
            raise ValueError('hook actual before/after witness bytes changed')
        before_text, after_text = before_path.read_text(), after_path.read_text()
        before, after = parse(before_text), parse(after_text)
        ae = {name for name, f in before['definitions'].items() if f['available_externally']}
        actual_ae = {name for name, f in imported[index]['definitions'].items() if f['available_externally']}
        if ae != actual_ae or not set(unit['required_prelink']).issubset(ae | set(unit['canonical'])):
            raise ValueError('postlink hook body visibility differs from actual import witness')
        changes = record.get('imports', [])
        if [v['canonical'] for v in changes] != sorted(ae):
            raise ValueError('hook changed other definitions or omitted an AE import')
        mapping = {}
        for change in changes:
            canonical = change['canonical']
            private = f'zil.import.{index}.{canonical}'
            if change['private'] != private or private in before['definitions'] or private in before['declarations']:
                raise ValueError('private import name collision or unexpected mapping')
            if canonical not in unit['allowed_original'] or type(change['direct_uses']) is not int or change['direct_uses'] < 0:
                raise ValueError('unbudgeted import or missing direct-Use evidence')
            # parse() already rejects every non-callee symbol occurrence in a
            # body and every address/global reference outside function bodies.
            # Subtract only the single definition-header occurrence.
            uses = sum(SYMBOL.findall(line.split(';', 1)[0]).count(canonical)
                       for line in before_text.splitlines()) - 1
            if uses != change['direct_uses']:
                raise ValueError('actual IR references differ from hook direct-Use count')
            if private not in after['definitions'] or 'internal' not in after['definitions'][private]['header'].split():
                raise ValueError('private implementation is not actual Internal definition')
            if canonical in after['definitions'] or canonical in after['declarations']:
                raise ValueError('renamed pure implementation retained a stale canonical reference')
            mapping[private] = canonical
        # The pass has no permission to change arithmetic, control flow,
        # attributes, thresholds or another function. Compare the whole module
        # after reversing precisely its approved names/linkage spelling.
        restored = SYMBOL.sub(lambda match: '@' + mapping.get(match.group(1), match.group(1)), after_text)
        if _normalize_header(before_text, ae) != _normalize_header(restored, ae):
            raise ValueError('hook changed more than approved import names/local linkage')
        records.append({'unit': index, 'imports': len(changes), 'receipt': identity(record_path),
                        'before': identity(before_path), 'after': identity(after_path),
                        'module_only_name_linkage_changes': True})
    return {'format': 'zil-thinlto-private-imports-validation/1', 'units': records,
            'manifest': identity(path), 'all_eight_actual_hooks_checked': True,
            'original_visibility_budget_unchanged': True}
