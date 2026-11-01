#!/usr/bin/env python3
"""Distinguish recorded, attempted-incomplete and genuinely unattempted requests."""
import hashlib
import json
from pathlib import Path


def request_ledger(root, cases, blocks, candidate, requests, reason, generated=False):
    root = Path(root)
    rows, errors = [], []
    if (root / 'rows.jsonl').is_file():
        for number, line in enumerate((root / 'rows.jsonl').read_text().splitlines(), 1):
            try:
                value = json.loads(line)
                if not isinstance(value, dict):
                    raise ValueError('row is not an object')
                rows.append(value)
            except ValueError as error:
                errors.append({'line': number, 'error': repr(error)})
    expected = {(case, block, arm, request) for case in cases for block in range(blocks)
                for arm in [candidate, 'rust'] for request, _ in requests}
    recorded = {(row.get('case'), row.get('block'), row.get('arm'), row.get('request')) for row in rows}
    not_run, incomplete = [], []
    for case in cases:
        for block in range(blocks):
            order = [candidate, 'rust'] if block % 2 == 0 else ['rust', candidate]
            for position, arm in enumerate(order):
                run = root / (f'{case}-O2-{block}-{position}-{arm}' if generated else f'{case}-{block}-{position}-{arm}')
                for request, _ in requests:
                    if (case, block, arm, request) in recorded:
                        continue
                    entry = {'case': case, 'block': block, 'position': position, 'arm': arm, 'request': request, 'reason': reason}
                    paths = [run / (request + suffix) for suffix in ['.stdout', '.stderr', '.interrupted.json']]
                    evidence = []
                    for path in paths:
                        if path.is_file():
                            contents = path.read_bytes()
                            evidence.append({'path': str(path), 'sha256': hashlib.sha256(contents).hexdigest(), 'bytes': len(contents)})
                    if evidence:
                        entry.update(status='AttemptedIncomplete', evidence=evidence,
                                     boundary='Compiler-request files exist without a complete row. No successful launch, exit, timing, or native outcome is inferred from this fact.')
                        incomplete.append(entry)
                    else:
                        entry['status'] = 'NotRun'
                        not_run.append(entry)
    interrupted = sum(row.get('status') == 'Interrupted' for row in rows)
    return {'observed_requests': len(rows), 'interrupted_requests': interrupted,
            'completed_requests': len(rows) - interrupted, 'attempted_incomplete_requests': len(incomplete),
            'attempted_requests': len(recorded & expected) + len(incomplete), 'not_run_requests': len(not_run),
            'invalid_requests': sum(not row.get('valid', False) for row in rows),
            'duplicate_request_keys': len(rows) - len(recorded), 'unexpected_request_keys': len(recorded - expected),
            'raw_row_parse_errors': errors, 'not_run_records': not_run, 'attempted_incomplete_records': incomplete}
