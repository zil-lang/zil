"""Read a frozen, standard-extracted evidence directory like a ZIP archive.

Only transport lives here. The caller supplies the existing path policy and
continues to own all manifest, SHA, index and restoration validation.
"""
import os
from pathlib import Path
import stat
from types import SimpleNamespace


def identity(info):
    return (info.st_dev, info.st_ino, info.st_mode, info.st_size, info.st_mtime_ns)


class DirectoryReader:
    def __init__(self, root, safe_path):
        self.root = Path(root)
        self.safe_path = safe_path
        self.descriptor = None
        self.files = {}
        self.directories = {}

    def inventory(self):
        files, directories, pending = {}, {}, [self.root]
        while pending:
            directory = pending.pop()
            info = directory.lstat()
            if not stat.S_ISDIR(info.st_mode):
                raise ValueError("evidence directory is not a real directory: " + str(directory))
            relative = directory.relative_to(self.root).as_posix()
            directories[relative] = identity(info)
            with os.scandir(directory) as entries:
                for entry in entries:
                    path = Path(entry.path)
                    name = path.relative_to(self.root).as_posix()
                    self.safe_path(name)
                    kind = entry.stat(follow_symlinks=False)
                    if stat.S_ISDIR(kind.st_mode):
                        pending.append(path)
                    elif stat.S_ISREG(kind.st_mode):
                        files[name] = identity(kind)
                    else:
                        raise ValueError("evidence symlink or nonregular entry: " + str(path))
            if identity(directory.lstat()) != directories[relative]:
                raise ValueError("evidence directory changed while listing: " + str(directory))
        return files, directories

    def __enter__(self):
        self.files, self.directories = self.inventory()
        # Anchor reads through directory descriptors on platforms that support
        # openat/no-follow. Elsewhere every ancestor is checked and the opened
        # file's real identity is checked before reading any payload bytes.
        if (os.open in os.supports_dir_fd and hasattr(os, "O_NOFOLLOW")
                and hasattr(os, "O_DIRECTORY")):
            descriptor = os.open(self.root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
            if identity(os.fstat(descriptor)) != self.directories["."]:
                os.close(descriptor)
                raise ValueError("evidence root changed while opening")
            self.descriptor = descriptor
        return self

    def __exit__(self, kind, value, traceback):
        try:
            if kind is None and self.inventory() != (self.files, self.directories):
                raise ValueError("evidence directory changed during reading")
        finally:
            if self.descriptor is not None:
                os.close(self.descriptor)
                self.descriptor = None

    def infolist(self):
        return [SimpleNamespace(filename=name) for name in sorted(self.files)]

    def checked_path(self, parts):
        current = self.root
        for index, part in enumerate(parts):
            expected_directory = current.relative_to(self.root).as_posix()
            if identity(current.lstat()) != self.directories[expected_directory]:
                raise ValueError("evidence ancestor changed or became a symlink: " + str(current))
            current = current / part
            if index == len(parts) - 1:
                break
        return current

    def open_member(self, parts):
        if self.descriptor is None:
            path = self.checked_path(parts)
            if not stat.S_ISREG(path.lstat().st_mode):
                raise ValueError("evidence member is not a regular file: " + str(path))
            return path.open("rb")
        descriptor = os.dup(self.descriptor)
        try:
            for part in parts[:-1]:
                child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW,
                                dir_fd=descriptor)
                os.close(descriptor)
                descriptor = child
            member = os.open(parts[-1], os.O_RDONLY | os.O_NOFOLLOW, dir_fd=descriptor)
            return os.fdopen(member, "rb")
        finally:
            os.close(descriptor)

    def read(self, name):
        parts = self.safe_path(name).parts
        expected = self.files[name]
        with self.open_member(parts) as stream:
            if identity(os.fstat(stream.fileno())) != expected:
                raise ValueError("evidence member changed before reading: " + name)
            # Reject a growing file without an unbounded read.
            data = stream.read(expected[3] + 1)
            if len(data) != expected[3] or identity(os.fstat(stream.fileno())) != expected:
                raise ValueError("evidence member changed while reading: " + name)
        return data
