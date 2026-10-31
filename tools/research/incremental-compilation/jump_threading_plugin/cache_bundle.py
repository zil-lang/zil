"""Private research cache: verified ZIP_STORED bundles with atomic per-key publication."""
import hashlib
import json
import os
import tempfile
import zipfile
from pathlib import Path


NAMES = {'receipt.json', 'object.o', 'optimized.bc'}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def checked_payload(receipt, obj, bitcode, material=None):
    if not isinstance(receipt, dict) or not isinstance(receipt.get('material'), dict):
        raise ValueError('cache receipt is not an object with material')
    if material is not None and receipt['material'] != material:
        raise ValueError('cache material mismatch')
    if receipt.get('object_sha256') != digest(obj) or receipt.get('bitcode_sha256') != digest(bitcode):
        raise ValueError('cache payload hash mismatch')
    if not bitcode.startswith(b'BC\xc0\xde'):
        raise ValueError('cache bitcode header mismatch')
    return receipt, obj, bitcode


def read_bundle(path, material):
    archive_bytes = path.stat().st_size
    with zipfile.ZipFile(path) as archive:
        members = archive.infolist()
        if len(members) != 3 or {x.filename for x in members} != NAMES:
            raise ValueError('cache bundle members mismatch')
        if any(x.compress_type != zipfile.ZIP_STORED or x.file_size != x.compress_size or
               x.file_size > archive_bytes for x in members):
            raise ValueError('cache bundle storage mismatch')
        receipt = json.loads(archive.read('receipt.json'))
        return checked_payload(receipt, archive.read('object.o'), archive.read('optimized.bc'), material)


def publish_bundle(pending, destination):
    raw_receipt = (pending/'receipt.json').read_bytes()
    receipt, obj, bitcode = checked_payload(json.loads(raw_receipt),
        (pending/'object.o').read_bytes(), (pending/'optimized.bc').read_bytes())
    key = digest(json.dumps(receipt['material'], sort_keys=True, separators=(',', ':')).encode())
    if pending.name != key or destination.name != key+'.zip':
        raise ValueError('pending material/action key mismatch')
    destination.parent.mkdir(parents=True, exist_ok=True)
    descriptor, name = tempfile.mkstemp(prefix='.'+destination.stem+'.pending-', suffix='.zip', dir=destination.parent)
    temporary = Path(name)
    try:
        with os.fdopen(descriptor, 'w+b') as stream:
            with zipfile.ZipFile(stream, 'w', compression=zipfile.ZIP_STORED) as archive:
                archive.writestr('receipt.json', raw_receipt)
                archive.writestr('object.o', obj)
                archive.writestr('optimized.bc', bitcode)
            stream.flush()
            os.fsync(stream.fileno())
        # Same filesystem, one complete file: the old entry is never removed
        # before publication, and a failed replace leaves it intact.
        os.replace(temporary, destination)
    finally:
        if temporary.exists():
            temporary.unlink()
    return {'path': str(destination), 'bytes': destination.stat().st_size,
            'bundle_sha256': digest(destination.read_bytes()),
            'object_sha256': receipt['object_sha256'], 'bitcode_sha256': receipt['bitcode_sha256']}
