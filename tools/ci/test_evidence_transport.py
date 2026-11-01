import base64
import hashlib
import io
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
from unittest import mock

import evidence_transport as transport


class EvidenceTransportTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="zil-ci-evidence-")
        self.root = Path(self.temporary.name)
        self.runner = self.root / "runner"
        (self.runner / "receipts").mkdir(parents=True)
        (self.runner / "native-evidence/workspace").mkdir(parents=True)
        (self.runner / "receipts/result.json").write_bytes(b'{"status":"Fail"}\n')
        for name in ("one", "two"):
            path = self.runner / "native-evidence/workspace" / name
            path.write_bytes(b"\x7fELF\0\xff" + bytes(range(256)) * 128)
            path.chmod(0o755 if name == "one" else 0o644)
        (self.runner / "receipts/empty").touch()

    def tearDown(self):
        self.temporary.cleanup()

    def packed(self):
        archive = self.root / "evidence.tar.zst"
        transport.pack(self.runner, archive)
        return archive

    def test_lossless_native_modes_dedup_and_timestamped_log_roundtrip(self):
        archive = self.packed()
        log = io.StringIO()
        transport.emit(archive, log)
        raw = self.root / "actions.log"
        raw.write_text("\n".join("job\trecovery\t2026-11-02T00:00:00Z " + line
                                 for line in log.getvalue().splitlines()) + "\n")
        recovered = self.root / "recovered.tar.zst"
        transport.recover(raw, recovered)
        self.assertEqual(archive.read_bytes(), recovered.read_bytes())
        restored = self.root / "restored"
        index = transport.verify(recovered, restored)
        self.assertEqual(len(index["files"]), 4)
        self.assertEqual(len({entry["sha256"] for entry in index["files"]}), 3)
        for original in self.runner.rglob("*"):
            if original.is_file():
                copy = restored / original.relative_to(self.runner)
                self.assertEqual(copy.read_bytes(), original.read_bytes())
                self.assertEqual(copy.stat().st_mode, original.stat().st_mode)
        with self.assertRaises(ValueError):
            transport.verify(recovered, restored)

    def test_missing_corrupt_reordered_duplicate_and_incomplete_log_rejected(self):
        # Incompressible input ensures several numbered log records.
        (self.runner / "receipts/data").write_bytes(b"".join(
            hashlib.sha256(str(i).encode()).digest() for i in range(1000)))
        archive = self.packed()
        stream = io.StringIO()
        transport.emit(archive, stream)
        lines = stream.getvalue().splitlines()
        self.assertGreater(len(lines), 5)
        mutations = [lines[:2] + lines[3:], lines[:1] + [lines[2], lines[1]] + lines[3:],
                     lines[:2] + [lines[1]] + lines[2:], lines[:-1],
                     lines[:-1] + [transport.MARKER + " END " + "0" * 64],
                     lines + [lines[-1]]]
        broken = lines.copy()
        fields = broken[1].split(" ")
        data = bytearray(base64.b64decode(fields[-1]))
        data[0] ^= 1
        fields[-1] = base64.b64encode(data).decode()
        broken[1] = " ".join(fields)
        mutations.append(broken)
        for number, changed in enumerate(mutations):
            with self.subTest(number=number):
                log = self.root / f"bad-{number}.log"
                log.write_text("\n".join(changed) + "\n")
                with self.assertRaises(ValueError):
                    transport.recover(log, self.root / f"bad-{number}.zst")

    def raw_archive(self, index, members):
        raw = io.BytesIO()
        with tarfile.open(fileobj=raw, mode="w") as archive:
            encoded = json.dumps(index).encode()
            first = tarfile.TarInfo("INDEX.json")
            first.size = len(encoded)
            archive.addfile(first, io.BytesIO(encoded))
            for name, data in members:
                member = tarfile.TarInfo(name)
                member.size = len(data)
                archive.addfile(member, io.BytesIO(data))
        path = self.root / "malformed.zst"
        compressed = subprocess.run(["zstd", "-q", "-c"], input=raw.getvalue(),
                                    stdout=subprocess.PIPE, check=True).stdout
        path.write_bytes(compressed)
        return path

    def test_manifest_cannot_hide_missing_duplicate_or_corrupt_blobs(self):
        digest = hashlib.sha256(b"payload").hexdigest()
        index = {"format": transport.FORMAT, "files": [
            {"path": "receipts/result", "bytes": 7, "mode": 0o644, "sha256": digest}]}
        for members in ([], [("objects/" + digest, b"corrupt")],
                        [("objects/" + digest, b"payload")] * 2):
            with self.subTest(members=members), self.assertRaises(ValueError):
                transport.verify(self.raw_archive(index, members))

    def test_unsafe_paths_and_file_directory_conflicts_rejected(self):
        digest = hashlib.sha256(b"").hexdigest()
        for name in ("../outside", "/absolute", "a/../b", "a//b", ".", "a\\b", "a\0b"):
            index = {"format": transport.FORMAT, "files": [
                {"path": name, "bytes": 0, "mode": 0o644, "sha256": digest}]}
            with self.subTest(path=name), self.assertRaises(ValueError):
                transport.verify(self.raw_archive(index, [("objects/" + digest, b"")]))
        index["files"] = [{"path": name, "bytes": 0, "mode": 0o644, "sha256": digest}
                          for name in ("receipts", "receipts/file")]
        with self.assertRaises(ValueError):
            transport.verify(self.raw_archive(index, [("objects/" + digest, b"")]))

    def test_symlinks_bounds_and_output_input_overlap_rejected(self):
        link = self.runner / "receipts/link"
        link.symlink_to(self.runner / "receipts/result.json")
        with self.assertRaises(ValueError):
            transport.pack(self.runner, self.root / "linked.zst")
        link.unlink()
        with mock.patch.object(transport, "MAX_EXPANDED", 1), self.assertRaises(ValueError):
            transport.pack(self.runner, self.root / "bounded.zst")
        with self.assertRaises(ValueError):
            transport.pack(self.runner, self.runner / "receipts/archive.zst")


if __name__ == "__main__":
    unittest.main()
