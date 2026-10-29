import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import unittest
import zipfile

import compact_evidence as compact
import solid_evidence as solid


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

    def binary_container(self):
        logs = self.root / "binary-input"
        logs.mkdir()
        payloads = {"raw.stdin": b"\xff\0\x7fELF\x80",
                    "raw.stdout": bytes(range(256)), "raw.stderr": b"\0\xfe\xff",
                    "ct.bin": b"ZILCTPG\0\x1b\0\xff",
                    "cache.candidate": b"ZILRAW02\0\xff", "empty.log": b""}
        for name, data in payloads.items():
            (logs / name).write_bytes(data)
        (logs / "program.exe").write_bytes(b"MZ\0\xff")
        output = self.root / "binary.zip"
        compact.pack(self.inputs([{"component": "root", "id": "binary", "kind": "directory",
                                  "path": str(logs)}]), output, 1_000_000)
        return output, payloads

    def extracted_directory(self, source):
        output = self.root / "content.tar.xz"
        result = solid.convert(source, output)
        self.assertLessEqual(result["bytes"], 50_000_000)
        destination = self.root / "content"
        destination.mkdir()
        tar = shutil.which("tar")
        if tar is None:
            self.skipTest("standard tar is required for extraction integration")
        subprocess.run([tar, "-xf", str(output), "-C", str(destination)], check=True,
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        return destination

    def test_solid_standard_tar_directory_restores_binary_io_and_manifested_tools(self):
        source, payloads = self.binary_container()
        original_sha = compact.file_digest(source)
        contents = self.extracted_directory(source)
        self.assertEqual(compact.verify(contents), compact.verify(source))
        with zipfile.ZipFile(source) as archive:
            names = set(archive.namelist())
            extracted = {path.relative_to(contents).as_posix() for path in contents.rglob("*")
                         if path.is_file()}
            self.assertEqual(extracted, names)
            for name in names:
                self.assertEqual((contents / name).read_bytes(), archive.read(name))
            manifest = json.loads(archive.read("MANIFEST.json"))
        for tool in ("compact_evidence.py", "directory_evidence.py", "solid_evidence.py"):
            self.assertEqual(manifest[tool]["sha256"], compact.file_digest(contents / tool))
        # Run the delivered entry point, with bytecode suppression not supplied
        # by the test environment. It must not dirty its manifested directory.
        environment = os.environ.copy()
        environment.pop("PYTHONDONTWRITEBYTECODE", None)
        subprocess.run([sys.executable, str(contents / "compact_evidence.py"), "verify",
                        str(contents)], env=environment, check=True,
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.assertFalse((contents / "__pycache__").exists())
        restored = self.root / "directory-restored"
        compact.restore(contents, restored, "root")
        for name, data in payloads.items():
            self.assertEqual((restored / "components/root/binary" / name).read_bytes(), data)
        self.assertFalse((restored / "components/root/binary/program.exe").exists())
        self.assertEqual(compact.file_digest(source), original_sha)
        with self.assertRaises(FileExistsError):
            compact.restore(contents, restored)
        with self.assertRaisesRegex(ValueError, "outside the frozen"):
            compact.restore(contents, contents / "do-not-create")
        self.assertFalse((contents / "do-not-create").exists())

    def test_tampered_directory_is_rejected_before_restore_creates_destination(self):
        source, _ = self.binary_container()
        contents = self.extracted_directory(source)
        index = compact.verify(contents)
        (contents / index["files"][0]["stored_path"]).write_bytes(b"tampered")
        destination = self.root / "must-not-exist"
        with self.assertRaisesRegex(ValueError, "content mismatch"):
            compact.verify(contents)
        with self.assertRaisesRegex(ValueError, "content mismatch"):
            compact.restore(contents, destination)
        self.assertFalse(destination.exists())

    def test_directory_reuses_index_path_policy_before_restoring(self):
        source, _ = self.binary_container()
        contents = self.extracted_directory(source)
        index = compact.verify(contents)
        index["files"][0]["path"] = "../escape"
        data = compact.json_bytes(index)
        (contents / "INDEX.json").write_bytes(data)
        manifest = json.loads((contents / "MANIFEST.json").read_bytes())
        manifest["INDEX.json"] = {"bytes": len(data), "sha256": compact.digest(data)}
        (contents / "MANIFEST.json").write_bytes(compact.json_bytes(manifest))
        destination = self.root / "must-not-exist"
        with self.assertRaisesRegex(ValueError, "unsafe evidence path"):
            compact.restore(contents, destination)
        self.assertFalse(destination.exists())
        self.assertFalse((self.root / "escape").exists())

    def test_directory_symlinks_and_nonregular_entries_are_not_followed(self):
        source, _ = self.binary_container()
        contents = self.extracted_directory(source)
        linked_root = self.root / "linked-root"
        try:
            linked_root.symlink_to(contents, target_is_directory=True)
        except (OSError, NotImplementedError) as error:
            self.skipTest("symlinks unavailable: " + str(error))
        with self.assertRaisesRegex(ValueError, "not a real directory"):
            compact.verify(linked_root)
        index = compact.verify(contents)
        member = contents / index["files"][0]["stored_path"]
        original = member.read_bytes()
        external = self.root / "external-object"
        external.write_bytes(original)
        member.unlink()
        member.symlink_to(external)
        destination = self.root / "must-not-exist"
        with self.assertRaisesRegex(ValueError, "symlink or nonregular"):
            compact.restore(contents, destination)
        self.assertFalse(destination.exists())
        self.assertEqual(external.read_bytes(), original)
        member.unlink()
        member.write_bytes(original)
        directory = member.parent
        real_directory = directory.with_name(directory.name + "-actual")
        directory.rename(real_directory)
        directory.symlink_to(real_directory, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, "symlink or nonregular"):
            compact.verify(contents)
        directory.unlink()
        real_directory.rename(directory)
        with self.assertRaisesRegex(ValueError, "symlink ancestors"):
            compact.restore(contents, linked_root / "not-created")
        self.assertFalse((contents / "not-created").exists())
        if hasattr(os, "mkfifo"):
            os.mkfifo(contents / "pipe")
            with self.assertRaisesRegex(ValueError, "symlink or nonregular"):
                compact.verify(contents)

    def test_solid_cap_failure_preserves_exact_partial_and_existing_outputs(self):
        source, _ = self.binary_container()
        original = source.read_bytes()
        output = self.root / "too-small.tar.xz"
        with self.assertRaisesRegex(ValueError, "exceeds byte cap"):
            solid.convert(source, output, max_bytes=1)
        self.assertFalse(output.exists())
        pending = output.with_name(output.name + ".partial")
        preserved = pending.read_bytes()
        with zipfile.ZipFile(source) as left, tarfile.open(pending, "r:xz") as right:
            self.assertEqual([member.name for member in right], left.namelist())
            for member in right:
                self.assertTrue(member.isfile())
                self.assertEqual(right.extractfile(member).read(), left.read(member.name))
        with self.assertRaises(FileExistsError):
            solid.convert(source, output)
        self.assertEqual(pending.read_bytes(), preserved)
        existing = self.root / "existing.tar.xz"
        existing.write_bytes(b"must stay")
        with self.assertRaises(FileExistsError):
            solid.convert(source, existing)
        self.assertEqual(existing.read_bytes(), b"must stay")
        self.assertEqual(source.read_bytes(), original)

    def test_solid_conversion_rejects_tampered_or_special_zip_members(self):
        source, _ = self.binary_container()
        for label, mode in (("symlink", stat.S_IFLNK), ("fifo", stat.S_IFIFO),
                            ("tampered", None)):
            with self.subTest(label=label):
                bad = self.root / (label + ".zip")
                with zipfile.ZipFile(source) as original, zipfile.ZipFile(bad, "x") as modified:
                    for entry in original.infolist():
                        data = original.read(entry)
                        if entry.filename.startswith("records/"):
                            if mode is None:
                                data = b"changed"
                            else:
                                entry.create_system = 3
                                entry.external_attr = (mode | 0o644) << 16
                        modified.writestr(entry, data)
                if mode is not None:
                    # Existing byte-only ZIP verification is deliberately not
                    # redefined; the new converter enforces regular members.
                    compact.verify(bad)
                output = self.root / (label + ".tar.xz")
                with self.assertRaisesRegex(ValueError, "content mismatch|nonregular"):
                    solid.convert(bad, output)
                self.assertFalse(output.exists())
                self.assertFalse(output.with_name(output.name + ".partial").exists())

    def test_solid_conversion_rejects_root_name_and_file_directory_collisions(self):
        source, _ = self.binary_container()
        for label in ("dot", "parent"):
            with self.subTest(label=label):
                bad = self.root / (label + ".zip")
                with zipfile.ZipFile(source) as original, zipfile.ZipFile(bad, "x") as modified:
                    manifest = json.loads(original.read("MANIFEST.json"))
                    if label == "dot":
                        manifest["."] = manifest.pop("README.md")
                    else:
                        manifest["README.md/child"] = {"bytes": 5, "sha256": compact.digest(b"child")}
                    for entry in original.infolist():
                        name, data = entry.filename, original.read(entry)
                        if label == "dot" and name == "README.md":
                            name = "."
                        if name == "MANIFEST.json":
                            data = compact.json_bytes(manifest)
                        modified.writestr(name, data)
                    if label == "parent":
                        modified.writestr("README.md/child", b"child")
                output = self.root / (label + ".tar.xz")
                with self.assertRaisesRegex(ValueError, "unsafe evidence path|parent directory"):
                    solid.convert(bad, output)
                self.assertFalse(output.exists())
                self.assertFalse(output.with_name(output.name + ".partial").exists())


if __name__ == "__main__":
    unittest.main()
