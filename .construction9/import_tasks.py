"""Restore exact reviewed Git objects; publish only the named task refs."""
import base64
import hashlib
import json
import pathlib
import subprocess

def git(*args):
    return subprocess.check_output(['git', *args], text=True).strip()

manifest = json.loads(pathlib.Path('.construction9/manifest.json').read_text())
payload = base64.b64decode(pathlib.Path('.construction9/tasks.bundle.b64').read_text(), validate=True)
assert hashlib.sha256(payload).hexdigest() == manifest['bundle_sha256']
bundle = pathlib.Path('.construction9/tasks.bundle')
bundle.write_bytes(payload)
subprocess.run(['git', 'bundle', 'verify', str(bundle)], check=True)
allowed = {'task/228', 'task/229', 'task/228-consumer', 'task/228-229'}
refspecs = []
receipts = []
for target in manifest['targets']:
    branch, expected = target['branch'], target['sha']
    assert branch in allowed and len(expected) == 40
    ref = 'refs/heads/' + branch
    local_ref = 'refs/construction9/' + branch
    subprocess.run(['git', 'fetch', '--no-tags', str(bundle), ref + ':' + local_ref], check=True)
    actual = git('rev-parse', local_ref)
    assert actual == expected
    old = git('ls-remote', '--heads', 'origin', ref)
    if old:
        old = old.split()[0]
        subprocess.run(['git', 'fetch', '--no-tags', 'origin', ref], check=True)
        subprocess.run(['git', 'merge-base', '--is-ancestor', old, actual], check=True)
    refspecs.append(actual + ':' + ref)
    receipts.append({'branch': branch, 'sha': actual, 'previous': old or None})
# No force, no main writes; either every fast-forward succeeds or none do.
subprocess.run(['git', 'push', '--atomic', 'origin', *refspecs], check=True)
for row in receipts:
    observed = git('ls-remote', '--heads', 'origin', 'refs/heads/' + row['branch']).split()[0]
    assert observed == row['sha']
pathlib.Path('construction9-publication.json').write_text(json.dumps({'status': 'Pass', 'bundle_sha256': manifest['bundle_sha256'], 'targets': receipts}, indent=2) + '\n')
print(json.dumps(receipts, indent=2))
