import json
from pathlib import Path
import tempfile
import unittest
import zipfile

import compact_evidence as compact


class CompactEvidenceTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="zil204-evidence-tool-")
        self.root = Path(self.temporary.name)

    def tearDown(self):
        self.temporary.cleanup()

    def inputs(self, sources, frozen=True):
        path = self.root / "inputs.json"
        path.write_text(json.dumps({"inputs_frozen": frozen, "sources": sources}))
        return path

    def source_archive(self, name, files):
        path = self.root / (name + ".zip")
        with zipfile.ZipFile(path, "x") as archive:
            for member, data in files.items():
                archive.writestr(member, data)
        return {"component": "history", "id": name, "kind": "zip", "path": str(path),
                "bytes": path.stat().st_size, "sha256": compact.file_digest(path)}

    def test_original_history_paths_deduplicate_and_restore_exact_bytes(self):
        sources = [self.source_archive("before", {"same.txt": b"same", "result.json": b"Fail"}),
                   self.source_archive("after", {"same.txt": b"same", "result.json": b"Pass"})]
        output = self.root / "result.zip"
        result = compact.pack(self.inputs(sources), output, 1_000_000)
        self.assertEqual(result["statistics"]["logical_files"], 4)
        self.assertEqual(result["statistics"]["unique_contents"], 3)
        destination = self.root / "restored"
        compact.restore(output, destination, "history")
        self.assertEqual((destination / "components/history/before/result.json").read_bytes(), b"Fail")
        self.assertEqual((destination / "components/history/after/result.json").read_bytes(), b"Pass")
        with self.assertRaises(FileExistsError):
            compact.restore(output, destination)
        original = output.read_bytes()
        with self.assertRaises(FileExistsError):
            compact.pack(self.inputs(sources), output, 1_000_000)
        self.assertEqual(output.read_bytes(), original)

    def test_directory_keeps_wire_empty_and_invalid_source_but_hashes_machine_product(self):
        root = self.root / "logs"
        root.mkdir()
        (root / "native").write_bytes(b"\x7fELF" + b"\0" * 28)
        (root / "malformed.wire").write_bytes(b"\0\xffbroken")
        (root / "candidate.bin").write_bytes(b"ZILMODP\0\x1b\0\xff")
        (root / "stdout").write_bytes(b"")
        (root / "invalid.zil").write_bytes(b"\xff\0")
        sources = [{"component": "root", "id": "failed", "kind": "directory", "path": str(root)}]
        output = self.root / "result.zip"
        result = compact.pack(self.inputs(sources), output, 1_000_000)
        self.assertEqual(result["statistics"]["logical_files"], 4)
        self.assertEqual(result["statistics"]["excluded_execution_products"], 1)
        index = compact.verify(output)
        self.assertEqual(index["excluded_execution_products"][0]["sha256"],
                         compact.digest((root / "native").read_bytes()))

    def test_binary_io_ct_and_candidate_evidence_restore_without_machine_products(self):
        root = self.root / "logs"
        root.mkdir()
        payloads = {
            "invalid.stdin": b"\xff\0",
            "native.stdout": bytes(range(256)),
            "native.stderr": b"\0\xff",
            "format-looking.stdin": b"\x7fELF\0input bytes",
            "program.bin": b"ZILCTPG\0\x1b\0\xff",
            "ir.bin": b"ZILCTIR\0\x1b\0\xff",
            "cache.candidate": b"ZILRAW02\0\xff",
        }
        for name, data in payloads.items():
            (root / name).write_bytes(data)
        (root / "native.exe").write_bytes(b"MZ" + b"\0" * 28)
        output = self.root / "result.zip"
        result = compact.pack(self.inputs([{"component": "root", "id": "binary",
                                            "kind": "directory", "path": str(root)}]),
                              output, 1_000_000)
        self.assertEqual(result["statistics"]["logical_files"], len(payloads))
        self.assertEqual(result["statistics"]["excluded_execution_products"], 1)
        restored = self.root / "restored"
        compact.restore(output, restored)
        for name, data in payloads.items():
            self.assertEqual((restored / "components/root/binary" / name).read_bytes(), data)
        self.assertFalse((restored / "components/root/binary/native.exe").exists())
        with self.assertRaises((ValueError, UnicodeDecodeError)):
            compact.evidence_kind("unknown.bin", b"\0\xffunclassified")

    def test_input_digest_failure_does_not_publish_archive(self):
        source = self.source_archive("source", {"run.log": b"original"})
        source["sha256"] = "0" * 64
        output = self.root / "result.zip"
        with self.assertRaisesRegex(ValueError, "identity mismatch"):
            compact.pack(self.inputs([source]), output, 1_000_000)
        self.assertFalse(output.exists())

    def test_unfrozen_input_and_unsafe_member_do_not_publish_archive(self):
        source = self.source_archive("source", {"../escape.txt": b"never restore"})
        output = self.root / "result.zip"
        with self.assertRaisesRegex(ValueError, "not yet frozen"):
            compact.pack(self.inputs([source], frozen=False), output, 1_000_000)
        self.assertFalse(output.with_name(output.name + ".partial").exists())
        with self.assertRaisesRegex(ValueError, "unsafe evidence path"):
            compact.pack(self.inputs([source]), output, 1_000_000)
        self.assertFalse(output.exists())
        self.assertFalse((self.root / "escape.txt").exists())

    def test_tampered_object_is_rejected_before_restore_creates_destination(self):
        source = self.source_archive("source", {"run.log": b"original"})
        output = self.root / "result.zip"
        compact.pack(self.inputs([source]), output, 1_000_000)
        corrupt = self.root / "corrupt.zip"
        with zipfile.ZipFile(output) as original, zipfile.ZipFile(corrupt, "x") as modified:
            for member in original.infolist():
                data = original.read(member)
                if member.filename.startswith("records/"):
                    data = b"tampered"
                modified.writestr(member.filename, data)
        destination = self.root / "must-not-exist"
        with self.assertRaisesRegex(ValueError, "content mismatch"):
            compact.restore(corrupt, destination)
        self.assertFalse(destination.exists())

    def test_compact_input_restores_original_logical_paths_and_retains_exclusion_hashes(self):
        logs = self.root / "logs"
        logs.mkdir()
        (logs / "result.log").write_bytes(b"original failure\n")
        (logs / "program").write_bytes(b"\x7fELF" + b"\0" * 28)
        nested = self.root / "nested.zip"
        compact.pack(self.inputs([{"id": "failed", "component": "root", "kind": "directory",
                                   "path": str(logs)}]), nested, 1_000_000)
        copied = self.source_archive("copied", {"same.log": b"original failure\n"})
        aggregate = self.root / "aggregate.zip"
        nested_source = {"id": "failed-archive", "component": "root", "kind": "compact",
                         "path": str(nested), "bytes": nested.stat().st_size,
                         "sha256": compact.file_digest(nested)}
        compact.pack(self.inputs([nested_source, copied]), aggregate, 1_000_000)
        index = compact.verify(aggregate)
        originals = [item for item in index["files"] if item["sha256"] == compact.digest(b"original failure\n")]
        self.assertEqual(len(originals), 2)
        self.assertEqual(len({item["stored_path"] for item in originals}), 1)
        self.assertEqual(index["excluded_execution_products"][0]["path"], "failed/program")
        destination = self.root / "restored"
        compact.restore(aggregate, destination, "root")
        self.assertEqual((destination / "components/root/failed-archive/failed/result.log").read_bytes(),
                         b"original failure\n")
        self.assertFalse((destination / "components/history").exists())
        self.assertFalse((destination / "components/root/failed-archive/failed/program").exists())


if __name__ == "__main__":
    unittest.main()
