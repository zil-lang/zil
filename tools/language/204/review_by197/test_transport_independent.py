"""Independent transport checks; no compiler or packaged test is executed."""
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import stat
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest import mock
import zipfile

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import compact_evidence as compact
from directory_evidence import DirectoryReader
import solid_evidence as solid


def sha(data):
    return hashlib.sha256(data).hexdigest()


class IndependentTransportTest(unittest.TestCase):
    def setUp(self):
        recorded = os.environ.get("ZIL_DELIVERY_REVIEW_DIR")
        self.temporary = None
        if recorded:
            self.root = Path(recorded) / self.id().rsplit(".", 1)[1]
            self.root.mkdir(parents=True, exist_ok=False)
        else:
            self.temporary = tempfile.TemporaryDirectory(prefix="zil204-independent-transport-")
            self.root = Path(self.temporary.name)

    def tearDown(self):
        if self.temporary:
            self.temporary.cleanup()

    def prepare(self, large=False):
        logs = self.root / "original-records"
        logs.mkdir()
        payloads = {
            "before.json": b'{"actual_commit":"original","status":"Fail"}\n',
            "after.json": b'{"actual_commit":"fixed","status":"Pass"}\n',
            "empty.log": b"",
            "binary.stdin": b"\x7fELF\0\xffinput, not an executable",
            "bad.wire": bytes(range(256)) * (8200 if large else 3) + b"\0\xffend",
            "same.log": b"unchanged input\n",
            "duplicate.log": b"unchanged input\n",
        }
        for name, data in payloads.items():
            (logs / name).write_bytes(data)
        product = b"\x7fELF" + b"\0" * 37
        (logs / "native.exe").write_bytes(product)
        config = self.root / "inputs.json"
        config.write_text(json.dumps({"inputs_frozen": True, "sources": [{
            "id": "original", "component": "review", "kind": "directory", "path": str(logs)
        }]}))
        source = self.root / "source.zip"
        compact.pack(config, source, 5_000_000)
        return source, payloads, product

    def extract_zip(self, source, name="content"):
        destination = self.root / name
        destination.mkdir()
        with zipfile.ZipFile(source) as original:
            for item in original.infolist():
                parts = PurePosixPath(item.filename).parts
                self.assertTrue(parts)
                self.assertFalse(PurePosixPath(item.filename).is_absolute())
                self.assertNotIn("..", parts)
                target = destination.joinpath(*parts)
                target.parent.mkdir(parents=True, exist_ok=True)
                with target.open("xb") as output:
                    output.write(original.read(item))
        return destination

    def first_object(self, directory):
        index = json.loads((directory / "INDEX.json").read_bytes())
        return index["files"][0]["stored_path"]

    def rewrite_member(self, source, name, transform):
        output = self.root / name
        with zipfile.ZipFile(source) as original:
            members = [(item, original.read(item)) for item in original.infolist()]
        members = transform(members)
        with zipfile.ZipFile(output, "x") as modified:
            for item, data in members:
                modified.writestr(item, data)
        return output

    def test_standard_tar_exact_members_and_extracted_cli(self):
        source, payloads, product = self.prepare(large=True)
        source_before = source.read_bytes()
        output = self.root / "evidence.tar.xz"
        receipt = solid.convert(source, output)
        self.assertEqual(source.read_bytes(), source_before)
        self.assertEqual(receipt["source_sha256"], sha(source_before))
        self.assertFalse(receipt["member_content_changed"])
        with zipfile.ZipFile(source) as original, tarfile.open(output, "r:xz") as archive:
            zipped = original.infolist()
            members = archive.getmembers()
            self.assertEqual([item.name for item in members], [item.filename for item in zipped])
            self.assertEqual(receipt["members_compared_byte_for_byte"], len(zipped))
            for member, original_item in zip(members, zipped):
                self.assertTrue(member.isfile())
                self.assertEqual(archive.extractfile(member).read(), original.read(original_item))
        extracted = self.root / "standard-tar-content"
        extracted.mkdir()
        subprocess.run(["tar", "-xf", str(output), "-C", str(extracted)], check=True,
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        # Run the shipped tool normally: disabling bytecode in the test process
        # must not hide accidental __pycache__ creation by its real CLI.
        environment = dict(os.environ)
        environment.pop("PYTHONDONTWRITEBYTECODE", None)
        environment.pop("PYTHONPYCACHEPREFIX", None)
        cli = extracted / "compact_evidence.py"
        checked = subprocess.run([sys.executable, str(cli), "verify", str(extracted)],
                                 env=environment, check=True, capture_output=True)
        self.assertEqual(json.loads(checked.stdout)["integrity"], "Pass")
        restored = self.root / "restored"
        subprocess.run([sys.executable, str(cli), "restore", str(extracted), str(restored),
                        "--component", "review"], env=environment, check=True, capture_output=True)
        for name, data in payloads.items():
            self.assertEqual((restored / "components/review/original" / name).read_bytes(), data)
        index = json.loads((extracted / "INDEX.json").read_bytes())
        self.assertEqual(index["excluded_execution_products"][0]["sha256"], sha(product))
        self.assertFalse((restored / "components/review/original/native.exe").exists())
        self.assertFalse(list(extracted.rglob("__pycache__")))
        self.assertEqual(compact.verify(extracted), compact.verify(source))

    def test_directory_payload_tamper_stops_before_destination(self):
        source, _, _ = self.prepare()
        content = self.extract_zip(source)
        (content / self.first_object(content)).write_bytes(b"changed input")
        destination = self.root / "must-not-exist"
        with self.assertRaises(ValueError):
            compact.restore(content, destination)
        self.assertFalse(destination.exists())

    def test_index_path_attack_with_valid_manifest_is_rejected(self):
        source, _, _ = self.prepare()
        content = self.extract_zip(source)
        index = json.loads((content / "INDEX.json").read_bytes())
        index["files"][0]["restore_path"] = "../outside"
        data = json.dumps(index).encode()
        (content / "INDEX.json").write_bytes(data)
        manifest = json.loads((content / "MANIFEST.json").read_bytes())
        manifest["INDEX.json"] = {"bytes": len(data), "sha256": sha(data)}
        (content / "MANIFEST.json").write_text(json.dumps(manifest))
        destination = self.root / "must-not-exist"
        with self.assertRaises(ValueError):
            compact.restore(content, destination)
        self.assertFalse(destination.exists())
        self.assertFalse((self.root / "outside").exists())

    def test_directory_symlinks_and_nonregular_members_are_rejected(self):
        source, _, _ = self.prepare()
        content = self.extract_zip(source)
        root_link = self.root / "root-link"
        root_link.symlink_to(content, target_is_directory=True)
        with self.assertRaises(ValueError):
            compact.verify(root_link)
        target = content / self.first_object(content)
        outside = self.root / "same-object-outside"
        target.rename(outside)
        target.symlink_to(outside)
        with self.assertRaises(ValueError):
            compact.verify(content)
        target.unlink()
        outside.rename(target)
        records = content / "records"
        outside_records = self.root / "outside-records"
        records.rename(outside_records)
        records.symlink_to(outside_records, target_is_directory=True)
        with self.assertRaises(ValueError):
            compact.verify(content)
        records.unlink()
        outside_records.rename(records)
        fifo = content / "fifo"
        os.mkfifo(fifo)
        with self.assertRaises(ValueError):
            compact.verify(content)
        fifo.unlink()
        compact.verify(content)

    def test_directory_open_identity_and_fallback_do_not_follow_replacement(self):
        source, _, _ = self.prepare()
        for fallback in (False, True):
            with self.subTest(fallback=fallback):
                content = self.extract_zip(source, "fallback" if fallback else "openat")
                name = self.first_object(content)
                target = content / name
                data = target.read_bytes()
                supports = set() if fallback else os.supports_dir_fd
                with mock.patch.object(os, "supports_dir_fd", supports):
                    with self.assertRaises(ValueError):
                        with DirectoryReader(content, compact.safe_path) as reader:
                            self.assertEqual(reader.read(name), data)
                            target.rename(self.root / ("old-object-" + str(fallback)))
                            target.write_bytes(data)  # Same SHA, a different real file.
                            reader.read(name)

    def test_dot_root_and_symlink_zip_members_are_rejected(self):
        source, _, _ = self.prepare()

        def dot_root(members):
            result = []
            for item, data in members:
                if item.filename == "README.md":
                    item.filename = "."
                elif item.filename == "MANIFEST.json":
                    manifest = json.loads(data)
                    manifest["."] = manifest.pop("README.md")
                    data = json.dumps(manifest).encode()
                result.append((item, data))
            return result

        def symlink(members):
            for item, _ in members:
                if item.filename == "README.md":
                    item.create_system = 3
                    item.external_attr = (stat.S_IFLNK | 0o777) << 16
            return members

        for label, mutation in (("dot-root", dot_root), ("symlink", symlink)):
            with self.subTest(label=label):
                bad = self.rewrite_member(source, label + ".zip", mutation)
                output = self.root / (label + ".tar.xz")
                with self.assertRaises(ValueError):
                    solid.convert(bad, output)
                self.assertFalse(output.exists())

    def test_converter_and_restore_refuse_existing_or_symlink_destinations(self):
        source, _, _ = self.prepare()
        sentinel = self.root / "sentinel"
        sentinel.write_bytes(b"do not overwrite")
        for partial in (False, True):
            for linked in (False, True):
                label = f"destination-{partial}-{linked}.tar.xz"
                output = self.root / label
                occupied = output.with_name(output.name + ".partial") if partial else output
                if linked:
                    occupied.symlink_to(sentinel)
                else:
                    occupied.write_bytes(b"original destination")
                with self.assertRaises(FileExistsError):
                    solid.convert(source, output)
                self.assertEqual(sentinel.read_bytes(), b"do not overwrite")
                if not linked:
                    self.assertEqual(occupied.read_bytes(), b"original destination")
        source_link = self.root / "source-link.zip"
        source_link.symlink_to(source)
        with self.assertRaises(ValueError):
            solid.convert(source_link, self.root / "source-link-output.tar.xz")
        for partial in (False, True):
            output = self.root / f"dangling-{partial}.tar.xz"
            occupied = output.with_name(output.name + ".partial") if partial else output
            occupied.symlink_to(self.root / "nonexistent-target")
            with self.assertRaises(FileExistsError):
                solid.convert(source, output)
            self.assertTrue(occupied.is_symlink())
            self.assertFalse((self.root / "nonexistent-target").exists())
        content = self.extract_zip(source)
        for destination in (sentinel, content, content / "new-subdirectory"):
            with self.assertRaises((FileExistsError, ValueError)):
                compact.restore(content, destination)
        self.assertFalse((content / "new-subdirectory").exists())
        self.assertEqual(sentinel.read_bytes(), b"do not overwrite")
        compact.verify(content)

    def test_converter_byte_cap_has_exact_boundary_and_preserves_partial(self):
        source, _, _ = self.prepare()
        first = self.root / "first.tar.xz"
        solid.convert(source, first)
        size = first.stat().st_size
        too_small = self.root / "too-small.tar.xz"
        with self.assertRaises(ValueError):
            solid.convert(source, too_small, size - 1)
        self.assertFalse(too_small.exists())
        partial = too_small.with_name(too_small.name + ".partial")
        self.assertEqual(partial.read_bytes(), first.read_bytes())
        with self.assertRaises(FileExistsError):
            solid.convert(source, too_small, size)
        exact = self.root / "exact.tar.xz"
        solid.convert(source, exact, size)
        self.assertEqual(exact.read_bytes(), first.read_bytes())

    def test_actual_source_mutation_during_streaming_never_publishes(self):
        raw = self.root / "original.zip"
        with zipfile.ZipFile(raw, "x") as original:
            for number in range(1001):
                original.writestr(f"record-{number}.txt", f"unique payload {number}\n")
        config = self.root / "inputs.json"
        config.write_text(json.dumps({"inputs_frozen": True, "sources": [{
            "id": "many", "component": "review", "kind": "zip", "path": str(raw),
            "bytes": raw.stat().st_size, "sha256": sha(raw.read_bytes())
        }]}))
        source = self.root / "source.zip"
        compact.pack(config, source, 5_000_000)
        original_identity = sha(source.read_bytes())
        changed = []

        def mutate(progress):
            if progress["phase"] == "compress" and not changed:
                with source.open("ab") as output:
                    output.write(b"changed frozen source\n")
                changed.append(progress)

        output = self.root / "must-not-publish.tar.xz"
        with self.assertRaises(ValueError):
            solid.convert(source, output, progress=mutate)
        self.assertTrue(changed)
        self.assertNotEqual(sha(source.read_bytes()), original_identity)
        self.assertFalse(output.exists())
        self.assertTrue(output.with_name(output.name + ".partial").is_file())


if __name__ == "__main__":
    unittest.main(verbosity=2)
