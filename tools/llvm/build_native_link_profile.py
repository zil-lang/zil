#!/usr/bin/env python3
"""Install a closed Linux x86-64 GNU ld snapshot and its reviewable ZILLINK1 policy.

Discovery and the mandatory namespace probe are SDK installation work. This
program does not compile sources or benchmark requests. Unsupported discovery or
isolation never publishes a usable profile. The generated policy must be reviewed
and compiled into the consumer; a self-reported manifest is not trust authority.
"""
import argparse
from dataclasses import dataclass
import hashlib
import os
from pathlib import Path
import re
import shutil
import stat
import struct
import subprocess
import tempfile

TARGET = 'x86_64-unknown-linux-gnu'
VARIANTS = (('O0', False), ('O0', True), ('O2', False), ('O2', True))
VIRTUAL_DIRS = ('/', '/dev', '/inputs', '/out', '/proc', '/work')
MAX_FILES = 1024
MAX_BYTES = 512 * 1024 * 1024
SIMPLE_FLAGS = frozenset(('-pie', '--eh-frame-hdr', '--build-id', '--as-needed',
                          '--no-as-needed', '--start-group', '--end-group',
                          '-Bdynamic', '--enable-new-dtags', '--hash-style=gnu'))


class Unsupported(ValueError):
    """A complete supported installation could not be established."""


def require(value, message):
    if not value:
        raise Unsupported(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def absolute(value):
    require(isinstance(value, (str, Path)), 'path is not text')
    text = str(value)
    require(text.startswith('/') and not text.startswith('//') and '\0' not in text
            and '\n' not in text and '\r' not in text, 'invalid absolute Linux path: ' + repr(text))
    # Resolve before normalizing '..': an installed ancestor can be a symlink.
    return Path(text)


def safe_token(value):
    require(type(value) is str and value and '\0' not in value and '\n' not in value
            and '\r' not in value, 'invalid command token')
    return value


def hextext(value):
    return os.fsencode(safe_token(str(value))).hex()


def stamp(path):
    info = path.stat()
    return (info.st_dev, info.st_ino, info.st_mode, info.st_size, info.st_mtime_ns, info.st_ctime_ns)


@dataclass(frozen=True)
class Elf:
    kind: int
    interpreter: str | None
    needed: tuple
    soname: str | None


def elf(data):
    """Decode complete finite ELF64 headers, dependencies and section kinds."""
    require(len(data) >= 64 and data[:7] == b'\x7fELF\x02\x01\x01', 'not supported ELF64 little-endian')
    kind, machine = struct.unpack_from('<HH', data, 16)
    require(kind in (1, 2, 3) and machine == 62 and struct.unpack_from('<I', data, 20)[0] == 1,
            'not native x86-64 relocatable/executable/DSO')
    phoff, shoff = struct.unpack_from('<QQ', data, 32)
    ehsize, phsize, phnum, shsize, shnum, shnames = struct.unpack_from('<HHHHHH', data, 52)
    require(ehsize == 64 and shoff >= 64 and shsize == 64 and shnum > 0 and 0 < shnames < shnum
            and shoff + shnum * shsize <= len(data), 'unsupported/truncated ELF section inventory')
    sections = [struct.unpack_from('<IIQQQQIIQQ', data, shoff + i * 64) for i in range(shnum)]
    for section in sections:
        require(section[1] == 8 or section[4] + section[5] <= len(data), 'ELF section outside complete file')
    names = sections[shnames]
    require(names[1] == 3 and sections[0] == (0,) * 10, 'ELF null/name section inventory')
    strings = data[names[4]:names[4]+names[5]]
    require(strings.startswith(b'\0') and strings.endswith(b'\0'), 'complete ELF section-name strings')

    def string(table, offset):
        require(0 <= offset < len(table) and table.find(b'\0', offset) >= 0, 'ELF string outside table')
        return table[offset:table.find(b'\0', offset)].decode('utf-8', errors='strict')

    name_work = 1024 * 1024
    for index, section in enumerate(sections):
        name = string(strings, section[0])
        name_work -= len(name.encode()) + 1
        require(name_work >= 0, 'ELF section-name work limit')
        require(not name.startswith(('.gnu.lto_', '.gnu.debuglto_', '.gnu.offload', '.llvm', '.llvmbc'))
                and name not in ('.gnu_object_only', '.gnu.object_only')
                and 'bitcode' not in name and 'plugin' not in name, 'LTO/plugin object is unsupported')
        if kind != 1 or index == 0:
            continue
        require(section[1] in (1, 2, 3, 4, 7, 8, 9, 14, 15, 16, 17, 18)
                and section[2] & ~0x802006f7 == 0 and section[6] < shnum
                and (section[8] == 0 or section[8] & (section[8]-1) == 0), 'unsupported native relocatable section type/flags/link/alignment')
        require(section[1] != 8 or section[4] <= len(data), 'native NOBITS offset outside file')
        if section[1] == 2:
            require(section[9] == 24 and section[5] % 24 == 0 and sections[section[6]][1] == 3, 'native symbol table structure')
        elif section[1] in (4, 9):
            size = 24 if section[1] == 4 else 16
            require(section[9] == size and section[5] % size == 0 and section[7] < shnum
                    and sections[section[6]][1] == 2, 'native relocation table structure')
        elif section[1] in (14, 15, 16):
            require(section[9] == 8 and section[5] % 8 == 0, 'native array section structure')
        elif section[1] in (17, 18):
            require(section[9] == 4 and section[5] % 4 == 0 and sections[section[6]][1] == 2, 'native group/index section structure')
    interpreter = None
    if phnum:
        require(phsize == 56 and phoff >= 64 and phoff + phnum * phsize <= len(data), 'ELF program header bounds')
        for index in range(phnum):
            header = struct.unpack_from('<IIQQQQQQ', data, phoff + index * 56)
            require(header[2] + header[5] <= len(data), 'ELF segment outside file')
            if header[0] == 3:
                raw = data[header[2]:header[2]+header[5]]
                require(interpreter is None and raw.endswith(b'\0') and b'\0' not in raw[:-1], 'ambiguous ELF interpreter')
                interpreter = str(absolute(raw[:-1].decode()))
    needed, soname = [], None
    for section in sections:
        if section[1] != 6:
            continue
        require(section[6] < shnum and section[9] == 16 and section[5] % 16 == 0, 'ELF dynamic table bounds')
        table = sections[section[6]]
        dynamic_strings = data[table[4]:table[4]+table[5]]
        terminated = False
        for offset in range(section[4], section[4]+section[5], 16):
            tag, value = struct.unpack_from('<qQ', data, offset)
            if tag == 0:
                terminated = True
                break
            require(tag not in (0x6ffffefa, 0x6ffffefb, 0x6ffffefc, 0x7ffffffd, 0x7fffffff),
                    'ELF config/audit/filter/auxiliary dynamic loading unsupported')
            if tag in (1, 14, 15, 29):
                text = string(dynamic_strings, value)
                require(tag not in (15, 29) or not text, 'ELF RPATH/RUNPATH is outside the closed profile')
                if tag == 1:
                    require(re.fullmatch(r'[A-Za-z0-9_+.-]+', text), 'non-basename ELF dependency')
                    needed.append(text)
                elif tag == 14:
                    require(soname is None, 'duplicate ELF SONAME')
                    soname = text
    require(not any(s[1] == 6 for s in sections) or terminated, 'unterminated dynamic table')
    return Elf(kind, interpreter, tuple(needed), soname)


def native_archive(data):
    require(data.startswith(b'!<arch>\n'), 'not a regular native archive (thin archives unsupported)')
    offset = 8
    while offset < len(data):
        require(offset + 60 <= len(data) and data[offset+58:offset+60] == b'`\n', 'truncated native archive member')
        header = data[offset:offset+60]
        try:
            size = int(header[48:58].strip())
        except ValueError as error:
            raise Unsupported('invalid archive member size') from error
        require(size >= 0 and offset + 60 + size <= len(data), 'archive member outside complete file')
        name = header[:16].decode('ascii').strip()
        payload = data[offset+60:offset+60+size]
        if name not in ('/', '//', '/SYM64/'):
            require(not name.startswith('#1/'), 'BSD extended archive names unsupported')
            require(elf(payload).kind == 1, 'archive contains non-relocatable or plugin member')
        offset += 60 + size + (size & 1)
    require(offset == len(data), 'incomplete native archive')


def driver_words(line):
    """The finite double-quoted command syntax emitted by clang -###."""
    words, word, quoted, escaped, started = [], '', False, False, False
    for char in line:
        require(char not in ('\0', '\n', '\r'), 'multiline driver job')
        if escaped:
            word += char
            escaped = False
            started = True
        elif char == '\\':
            escaped = True
            started = True
        elif char == '"':
            quoted = not quoted
            started = True
        elif char.isspace() and not quoted:
            if started:
                words.append(word)
                word, started = '', False
        else:
            require(char != "'", 'single-quoted driver syntax unsupported')
            word += char
            started = True
    require(not quoted and not escaped, 'unterminated driver job')
    if started:
        words.append(word)
    require(words and all(words), 'empty driver job or argument')
    return words


def driver_job(stderr):
    jobs = [line.strip() for line in stderr.splitlines() if line.lstrip().startswith('"')]
    require(len(jobs) == 1, 'driver must emit exactly one absolute GNU ld job')
    argv = driver_words(jobs[0])
    absolute(argv[0])
    return argv


def search_directories(verbose):
    require('GNU ld' in verbose, 'linker is not GNU ld')
    roots = re.findall(r'SEARCH_DIR\("(=?/[^"\n]*)"\)', verbose)
    require(roots and len(roots) == verbose.count('SEARCH_DIR('), 'unrecognized GNU ld search directory syntax')
    paths = [absolute(root.removeprefix('=')) for root in roots]
    require(all('..' not in path.parts for path in paths), 'parent traversal in built-in GNU ld search root')
    return paths


def script_members(data):
    """Strict balanced glibc INPUT/GROUP/AS_NEEDED grammar; no INCLUDE/SEARCH_DIR."""
    text = data.decode('utf-8', errors='strict')
    clean = ''
    while '/*' in text:
        start = text.index('/*')
        end = text.find('*/', start + 2)
        require(end >= 0 and '/*' not in text[start+2:end], 'malformed linker script comment')
        clean += text[:start]
        text = text[end+2:]
    clean += text
    require('*/' not in clean, 'unmatched linker script comment')
    tokens = re.findall(r'[^\s(),;]+|[()]', clean)
    index, members = 0, []

    def group(nested=False):
        nonlocal index
        require(index < len(tokens) and tokens[index] == '(', 'script group requires opening parenthesis')
        index += 1
        count = 0
        while index < len(tokens) and tokens[index] != ')':
            token = tokens[index]
            index += 1
            if token == 'AS_NEEDED':
                group(True)
            else:
                require((token.startswith('/') and '..' not in Path(token).parts)
                        or re.fullmatch(r'-l[A-Za-z0-9_+.-]+', token)
                        or re.fullmatch(r'lib[A-Za-z0-9_+.-]+', token), 'unsupported linker script member: '+token)
                members.append(token)
            count += 1
        require(count > 0 and index < len(tokens) and tokens[index] == ')', 'empty/unbalanced linker script group')
        index += 1

    while index < len(tokens):
        token = tokens[index]
        index += 1
        if token == 'OUTPUT_FORMAT':
            require(tokens[index:index+3] == ['(', 'elf64-x86-64', ')'], 'unsupported linker script output format')
            index += 3
        elif token in ('INPUT', 'GROUP'):
            group()
        else:
            raise Unsupported('unknown linker script directive: '+token)
    require(members, 'script has no input members')
    return members


def dependency_rows(text):
    paths = []
    for line in text.splitlines():
        fields = line.split()
        if not fields or fields[0].startswith('linux-vdso.'):
            continue
        if len(fields) >= 3 and fields[1] == '=>':
            path = fields[2]
            require(path != 'not', 'unresolved runtime dependency')
        else:
            path = fields[0]
        paths.append(absolute(path))
    require(paths, 'empty/unrecognized dynamic loader dependency receipt')
    return paths


class Commands:
    def __call__(self, argv):
        # No inherited loader/compiler/search variables enter discovery.
        result = subprocess.run(list(map(str, argv)), env={'LC_ALL': 'C', 'PATH': '/usr/bin:/bin'},
                                cwd='/', capture_output=True, timeout=60)
        require(result.returncode == 0, 'installation command failed: '+repr(list(map(str, argv)))+'\n'+result.stderr.decode(errors='replace'))
        return result.stdout.decode('utf-8'), result.stderr.decode('utf-8')


class Inventory:
    def __init__(self, commands):
        self.commands = commands
        self.files = {}
        self.directories = set(VIRTUAL_DIRS)
        self.runtime_done = set()
        self.active_scripts = set()
        self.library_dirs = set()
        self.sonames = {}
        self.needed_hashes = {}

    def directory(self, path):
        path = Path(os.path.normpath(absolute(path)))
        while True:
            self.directories.add(str(path))
            if path == Path('/'):
                break
            path = path.parent

    def file(self, original):
        original = absolute(original)
        canonical = original.resolve(strict=True)
        require(canonical.is_file() and not canonical.is_symlink(), 'installed input is not a regular file')
        info = canonical.stat()
        require(stat.S_ISREG(info.st_mode) and not info.st_mode & 0o7000
                and info.st_size <= MAX_BYTES, 'unsupported file type/mode/size')
        before = stamp(canonical)
        data = canonical.read_bytes()
        require(stamp(canonical) == before and len(data) == info.st_size, 'installed file changed during capture')
        aliases = {str(canonical)}
        if '..' not in original.parts:
            aliases.add(str(original))
        for name in aliases:
            require(not any(name == virtual or name.startswith(virtual+'/') for virtual in VIRTUAL_DIRS if virtual != '/'), 'installed input collides with private runtime mount')
            item = {'source': canonical, 'size': len(data), 'mode': stat.S_IMODE(info.st_mode),
                    'sha256': digest(data), 'stamp': before}
            require(name not in self.files or self.files[name] == item, 'ambiguous alias or changing installed file')
            self.files[name] = item
            self.directory(Path(name).parent)
        require(len(self.files) <= MAX_FILES and sum(f['size'] for f in self.files.values()) <= MAX_BYTES,
                'closed native profile exceeds bounded installation size')
        return canonical, data

    def runtime(self, original):
        canonical, data = self.file(original)
        info = elf(data)
        require(info.kind in (2, 3), 'runtime tool/dependency is not executable or DSO')
        if info.soname:
            require(info.soname not in self.sonames or self.sonames[info.soname] == digest(data), 'different runtime DSOs have the same SONAME')
            self.sonames[info.soname] = digest(data)
        if info.soname or info.interpreter is None:
            directories = {str(canonical.parent)}
            if '..' not in absolute(original).parts:
                directories.add(str(absolute(original).parent))
            require(all(':' not in directory for directory in directories), 'colon in runtime library path')
            self.library_dirs.update(directories)
        if canonical in self.runtime_done:
            return info
        self.runtime_done.add(canonical)
        if info.interpreter:
            self.runtime(info.interpreter)
        if info.needed:
            stdout, stderr = self.commands(['/usr/bin/ldd', str(canonical)])
            dependencies = dependency_rows(stdout)
            found = {}
            for path in dependencies:
                dependency = self.runtime(path)
                identity = self.files[os.path.normpath(path)]['sha256']
                found[path.name] = identity
                if dependency.soname:
                    found[dependency.soname] = identity
            require(set(info.needed) <= set(found), 'ldd closure omitted an ELF DT_NEEDED dependency')
            for name in info.needed:
                require(name not in self.needed_hashes or self.needed_hashes[name] == found[name],
                        'inconsistent runtime dependency resolution')
                self.needed_hashes[name] = found[name]
        return info

    def verify_loader_lookup(self):
        # --inhibit-cache does not disable the loader's fallback directories.
        # Every needed basename must therefore resolve in the explicit profile
        # library path, with exactly the bytes observed by ldd, before fallback.
        for name, identity in self.needed_hashes.items():
            candidates = [str(Path(directory)/name) for directory in sorted(self.library_dirs)]
            selected = next((self.files[path] for path in candidates if path in self.files), None)
            require(selected is not None and selected['sha256'] == identity,
                    'explicit snapshot library path cannot reproduce loader closure: '+name)

    def library(self, name, searches):
        require(re.fullmatch(r'[A-Za-z0-9_+.-]+', name), 'unsupported -l name')
        candidates = [root/('lib'+name+suffix) for root in searches for suffix in ('.so', '.a')]
        selected = [path for path in candidates if path.is_file()]
        require(selected, 'unresolved library: '+name)
        for path in selected:
            self.link_input(path, searches)

    def link_input(self, original, searches):
        canonical, data = self.file(original)
        if data.startswith(b'\x7fELF'):
            info = elf(data)
            require(info.kind in (1, 3), 'executable ELF is not a supported GNU ld link input')
            if info.kind in (2, 3):
                self.runtime(original)
            return
        if data.startswith(b'!<arch>\n'):
            native_archive(data)
            return
        require(canonical not in self.active_scripts, 'recursive linker script cycle')
        self.active_scripts.add(canonical)
        try:
            for member in script_members(data):
                if member.startswith('/'):
                    self.link_input(member, searches)
                elif member.startswith('-l'):
                    self.library(member[2:], searches)
                else:
                    candidates = [absolute(original).parent/member, canonical.parent/member]
                    candidates += [root/member for root in searches]
                    selected = list(dict.fromkeys(path for path in candidates if path.is_file()))
                    require(selected, 'unresolved relative linker script member: '+member)
                    for path in selected:
                        self.link_input(path, searches)
        finally:
            self.active_scripts.remove(canonical)


def close_job(argv, placeholder, output, ld, default_searches, inventory):
    require(absolute(argv[0]).resolve(strict=True) == ld.resolve(strict=True), 'driver selected an unexpected linker')
    inventory.runtime(argv[0])
    searches = []
    for arg in argv[1:]:
        if arg.startswith('-L'):
            require(len(arg) > 2, 'split/empty -L is unsupported')
            directory = absolute(arg[2:]).resolve()
            searches.append(directory)
    searches += default_searches
    searches = list(dict.fromkeys(searches))
    for directory in searches:
        inventory.directory(directory)
    result, objects, outputs, index, loader = [('ARG', str(ld))], 0, 0, 1, None
    while index < len(argv):
        arg = safe_token(argv[index])
        index += 1
        if arg == str(placeholder):
            result.append(('OBJECTS', None))
            objects += 1
        elif arg == '-o':
            require(index < len(argv) and argv[index] == str(output), 'unexpected GNU ld output')
            index += 1
            result += [('ARG', '-o'), ('OUTPUT', None)]
            outputs += 1
        elif arg.startswith('-L'):
            result.append(('ARG', '-L'+str(absolute(arg[2:]).resolve())))
        elif arg.startswith('-l'):
            inventory.library(arg[2:], searches)
            result.append(('ARG', arg))
        elif arg.startswith('/'):
            inventory.link_input(arg, searches)
            result.append(('ARG', str(absolute(arg).resolve(strict=True))))
        elif arg in ('-m', '-dynamic-linker'):
            require(index < len(argv), 'missing GNU ld option value')
            value = safe_token(argv[index])
            index += 1
            if arg == '-m':
                require(value == 'elf_x86_64', 'unsupported GNU ld emulation')
            else:
                require(loader is None, 'duplicate target interpreter')
                loader = str(absolute(value))
                inventory.runtime(loader)
            result += [('ARG', arg), ('ARG', value)]
        elif arg in SIMPLE_FLAGS:
            result.append(('ARG', arg))
        else:
            raise Unsupported('unsupported GNU ld option/input: '+arg)
    require(objects == outputs == 1 and loader is not None, 'exactly one OBJECTS/OUTPUT and target interpreter required')
    return result, loader


def manifest_bytes(inventory, loader, bwrap, variants):
    require(set(variants) == set(VARIANTS), 'all four generic native variants required')
    lines = ['ZILLINK1', 'TARGET '+hextext(TARGET), 'LOADER '+hextext(loader), 'BWRAP '+hextext(bwrap)]
    for path in sorted(inventory.library_dirs):
        lines.append('LIBDIR '+hextext(path))
    for optimization, threads in VARIANTS:
        lines.append(f'VARIANT {optimization} {int(threads)}')
        argv = variants[(optimization, threads)]
        require(sum(kind == 'OBJECTS' for kind, value in argv) == sum(kind == 'OUTPUT' for kind, value in argv) == 1,
                'canonical variant marker cardinality')
        for kind, value in argv:
            require(kind in ('ARG', 'OBJECTS', 'OUTPUT') and (value is not None) == (kind == 'ARG'), 'canonical variant token')
            lines.append('ARG '+hextext(value) if kind == 'ARG' else kind)
        lines.append('ENDVARIANT')
    for path, item in sorted(inventory.files.items()):
        lines.append(f"FILE {hextext(path)} {item['size']} {item['mode']:04o} {item['sha256']}")
    for path in sorted(inventory.directories):
        lines.append('DIR '+hextext(path))
    lines.append('END')
    return ('\n'.join(lines)+'\n').encode('ascii')


def materialize(inventory, destination):
    require(not destination.exists() and not destination.is_symlink(), 'snapshot destination already exists')
    destination.mkdir(mode=0o755)
    for directory in sorted(inventory.directories, key=lambda p: (p.count('/'), p)):
        (destination/directory.lstrip('/')).mkdir(mode=0o755, parents=True, exist_ok=True)
    for name, item in sorted(inventory.files.items()):
        source = item['source']
        require(stamp(source) == item['stamp'], 'installed file changed before snapshot copy')
        data = source.read_bytes()
        require(stamp(source) == item['stamp'] and len(data) == item['size'] and digest(data) == item['sha256'],
                'installed file changed while materializing snapshot')
        path = destination/name.lstrip('/')
        with path.open('xb') as stream:
            stream.write(data)
        path.chmod(item['mode'])
    actual_files, actual_dirs = set(), {'/'}
    for path in destination.rglob('*'):
        require(not path.is_symlink(), 'materialization introduced a symlink')
        relative = '/'+str(path.relative_to(destination))
        (actual_dirs if path.is_dir() else actual_files).add(relative)
    require(actual_files == set(inventory.files) and actual_dirs == inventory.directories, 'exact snapshot full file/directory inventory')


def isolation_command(root, inventory, loader, bwrap, linker):
    """The outer loader resolves bwrap exclusively from snapshot library paths."""
    root = root.resolve()
    outer_dirs = ':'.join(str(root/path.lstrip('/')) for path in sorted(inventory.library_dirs))
    inner_dirs = ':'.join(sorted(inventory.library_dirs))
    return [str(root/loader.lstrip('/')), '--inhibit-cache', '--glibc-hwcaps-mask', '', '--library-path', outer_dirs,
            str(root/bwrap.lstrip('/')), '--unshare-user', '--unshare-pid', '--unshare-net', '--die-with-parent',
            '--ro-bind', str(root), '/', '--proc', '/proc', '--dev', '/dev', '--tmpfs', '/out',
            '--chdir', '/work', '--clearenv', '--setenv', 'LC_ALL', 'C',
            loader, '--inhibit-cache', '--glibc-hwcaps-mask', '', '--library-path', inner_dirs, str(linker), '--version']


def install(*, clang, ld, bwrap, placeholder, snapshot_root, policy_output, commands=None):
    commands = Commands() if commands is None else commands
    clang, ld, bwrap, placeholder = map(absolute, (clang, ld, bwrap, placeholder))
    require(not Path('/etc/ld.so.preload').exists() and not Path('/etc/ld.so.preload').is_symlink(),
            'host /etc/ld.so.preload must be absent')
    require(all('..' not in tool.parts for tool in (clang, ld, bwrap)), 'configured tool paths must have no parent traversal')
    require(all(path.is_file() for path in (clang, ld, bwrap, placeholder)), 'missing trusted tool or placeholder object')
    require(elf(placeholder.read_bytes()).kind == 1, 'placeholder must be an existing ordinary native relocatable object')
    snapshot_root, policy_output = Path(snapshot_root).absolute(), Path(policy_output).absolute()
    require(not snapshot_root.exists() and not snapshot_root.is_symlink() and not policy_output.exists()
            and not policy_output.is_symlink() and policy_output.parent.is_dir(), 'fresh snapshot and policy output required')
    inventory = Inventory(commands)
    stdout, stderr = commands([ld, '--verbose'])
    searches = search_directories(stdout)
    ld_info = inventory.runtime(ld)
    wrap_info = inventory.runtime(bwrap)
    require(ld_info.interpreter is not None and ld_info.interpreter == wrap_info.interpreter,
            'GNU ld and bwrap must use the same explicit target ABI loader')
    loader = ld_info.interpreter
    loader_info = inventory.runtime(loader)
    require(loader_info.interpreter is None and not loader_info.needed,
            'outer ELF loader must have no implicit interpreter or runtime dependency')
    variants = {}
    with tempfile.TemporaryDirectory(prefix='zil-native-link-discover-') as directory:
        output = Path(directory)/'program'
        for optimization, threads in VARIANTS:
            argv = [clang, '-###', '--target='+TARGET, '-'+optimization, placeholder, '-o', output]
            if threads:
                argv.append('-pthread')
            stdout, stderr = commands(argv)
            variants[(optimization, threads)], target_loader = close_job(driver_job(stderr), placeholder, output, ld, searches, inventory)
            require(target_loader == loader, 'variant target interpreter differs from installed tool loader')
    inventory.verify_loader_lookup()
    policy = manifest_bytes(inventory, loader, str(bwrap), variants)
    snapshot_root.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.zil-native-link-stage-', dir=snapshot_root.parent) as directory:
        stage = Path(directory)/'root'
        materialize(inventory, stage)
        # Namespace failure (including restricted uid_map in nested sandboxes) is
        # Unsupported. There is deliberately no host-execution fallback.
        stdout, stderr = commands(isolation_command(stage, inventory, loader, str(bwrap), ld))
        require(stdout.startswith('GNU ld'), 'isolated complete snapshot failed GNU ld identity probe')
        require(not Path('/etc/ld.so.preload').exists() and not Path('/etc/ld.so.preload').is_symlink(),
                'host preload appeared during installation')
        stage.rename(snapshot_root)
        try:
            with policy_output.open('xb') as stream:
                stream.write(policy)
        except BaseException:
            # This snapshot was created by this invocation and cannot be
            # accepted without the separately published reviewable policy.
            shutil.rmtree(snapshot_root)
            raise
    return {'profile_id': digest(policy), 'snapshot_root': str(snapshot_root), 'policy': str(policy_output),
            'files': len(inventory.files), 'directories': len(inventory.directories), 'variants': len(variants)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('clang', 'ld', 'bwrap', 'placeholder-object', 'snapshot-root', 'policy-output'):
        parser.add_argument('--'+name, required=True, type=Path)
    args = parser.parse_args()
    try:
        result = install(clang=args.clang, ld=args.ld, bwrap=args.bwrap, placeholder=args.placeholder_object,
                         snapshot_root=args.snapshot_root, policy_output=args.policy_output)
    except (Unsupported, OSError, UnicodeError, subprocess.TimeoutExpired) as error:
        print('Unsupported: '+str(error), file=__import__('sys').stderr)
        return 2
    print('profile_id='+result['profile_id'])
    print('policy='+result['policy'])
    print('snapshot_root='+result['snapshot_root'])
    print('files='+str(result['files'])+' directories='+str(result['directories'])+' variants='+str(result['variants']))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
