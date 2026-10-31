"""Read-only launch inventory for the pinned native cosmic-panel config semantics.

This is deliberately a bounded supported subset, not a general RON/desktop-entry
interpreter. Ambiguous panel configuration selects stock before launching;
individual applet errors are diagnostic in ordinary startup. No user configuration
is written. See docs/COMPATIBILITY.md for the qualified scope.
"""
import ctypes
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import socket
import struct
import time
from collections import deque

from bundle_contract import identity, CONFIGURATION_RESOLVER, CONFIGURATION_SOURCE

RESOLVER = CONFIGURATION_RESOLVER
PREFIX = 'com.system76.CosmicPanel'
ID = re.compile(r'[A-Za-z0-9_.-]{1,200}')


class Changed(ValueError):
    pass


class Watch:
    """Watch dependencies before reading, including absence and replacements/ABA."""
    def __init__(self):
        self.lib = ctypes.CDLL(None, use_errno=True)
        self.fd = self.lib.inotify_init1(os.O_NONBLOCK | os.O_CLOEXEC)
        if self.fd < 0:
            raise OSError(ctypes.get_errno(), 'inotify unavailable')
        self.paths = {}
        self.registrations = set()
        self.optional_registrations = set()
        self.content_only = set()
        self.dirty = False

    def add(self, path, directory=False, *, entry=False, optional=False):
        path = Path(path)
        registration = (str(path), directory, entry)
        if entry:
            self._entry(path, directory)
        elif any(p.is_symlink() for p in (path, *path.parents)):
            raise ValueError('configuration_symlink_unsupported')
        else:
            self._add(path, directory)
        # Failed optional discovery must never become a required transfer.
        if optional and registration not in self.registrations:
            self.optional_registrations.add(registration)
        elif not optional:
            self.optional_registrations.discard(registration)
        self.registrations.add(registration)

    def _entry(self, path, directory):
        # Observe each link before reading it, including merged-/usr ancestors,
        # and the resolved target. Keep strict config-tree validation separate.
        if not path.is_absolute(): raise ValueError('configuration_relative_path')
        pending = deque(path.parts[1:]); resolved = Path(path.anchor); links = 0
        while pending:
            component = pending.popleft()
            if component == '..':
                resolved = resolved.parent
                continue
            candidate = resolved/component
            self._add(candidate, False)
            if candidate.is_symlink():
                links += 1
                if links > 40: raise ValueError('configuration_symlink_limit')
                target = Path(os.readlink(candidate))
                if target.is_absolute(): resolved = Path(target.anchor)
                pending.extendleft(reversed(target.parts[1:] if target.is_absolute() else target.parts))
            else:
                resolved = candidate
        self._add(resolved, directory)

    def add_content(self, path):
        """RPM WAL reads change bookkeeping without changing package data.

        Still detect writes, truncation, removal, replacement and ABA. Used for
        package database files only, never configuration or executable watches.
        """
        self.add(path)
        self.content_only.add(self._add(Path(path), False))

    def _add(self, path, directory):
        parent, name = (path, None) if directory and path.is_dir() else (path.parent, path.name)
        while not parent.is_dir():
            name, parent = parent.name, parent.parent
        mask = 0x2 | 0x4 | 0x8 | 0x40 | 0x80 | 0x100 | 0x200 | 0x400 | 0x800
        wd = self.lib.inotify_add_watch(self.fd, os.fsencode(parent), mask)
        if wd < 0:
            raise OSError(ctypes.get_errno(), 'configuration watch failed')
        self.paths.setdefault(wd, set()).add(name)
        if len(self.paths) > 4096:
            raise ValueError('configuration_watch_limit')
        return wd, name

    def changed(self):
        while self.fd >= 0:
            try:
                data = os.read(self.fd, 65536)
            except BlockingIOError:
                break
            if not data:
                break
            offset = 0
            while offset < len(data):
                wd, mask, _, size = struct.unpack_from('iIII', data, offset)
                name = os.fsdecode(data[offset+16:offset+16+size].split(b'\0')[0])
                watched = self.paths.get(wd, set())
                relevant = name in watched and ((wd, name) not in getattr(self, 'content_only', ())
                    or mask & (0x2 | 0x40 | 0x80 | 0x100 | 0x200))
                if mask & (0x4000 | 0x8000 | 0x400 | 0x800) or None in watched or relevant:
                    self.dirty = True
                offset += 16 + size
        return self.dirty

    def close(self):
        if self.fd >= 0:
            os.close(self.fd)
            self.fd = -1

    def __del__(self):
        self.close()


# Serialized COSMIC lists/options/tuples/strings and output variants, including
# whitespace, trailing commas and nested comments. Other RON forms fail closed.
def ron(text):
    tokens = []
    i = 0
    while i < len(text):
        if text[i].isspace(): i += 1; continue
        if text.startswith('//', i):
            end = text.find('\n', i); i = len(text) if end < 0 else end+1; continue
        if text.startswith('/*', i):
            depth = 1; i += 2
            while i < len(text) and depth:
                if text.startswith('/*', i): depth += 1; i += 2
                elif text.startswith('*/', i): depth -= 1; i += 2
                else: i += 1
            if depth: raise ValueError('ron_comment')
            continue
        if text[i] == '"':
            # COSMIC identifiers/paths are the supported ASCII subset. Reject
            # unsupported escape syntax instead of changing its interpretation.
            value, length = json.JSONDecoder().raw_decode(text[i:])
            if any(escape not in ('\\', '"', 'n', 'r', 't') for escape in re.findall(r'\\(.)', text[i:i+length])):
                raise ValueError('ron_string_escape_unsupported')
            tokens.append(('string', value)); i += length; continue
        match = re.match(r'[A-Za-z_][A-Za-z_0-9]*|[\[\](),]', text[i:])
        if not match: raise ValueError('ron_unsupported')
        tokens.append(match[0]); i += len(match[0])
    pos = 0
    def value(depth=0):
        nonlocal pos
        if depth > 12 or pos >= len(tokens): raise ValueError('ron_depth_or_end')
        token = tokens[pos]; pos += 1
        if isinstance(token, tuple): return token[1]
        if token in ('[', '('):
            end = ']' if token == '[' else ')'; items = []
            while pos < len(tokens) and tokens[pos] != end:
                items.append(value(depth+1))
                if pos < len(tokens) and tokens[pos] == ',': pos += 1
                elif pos >= len(tokens) or tokens[pos] != end: raise ValueError('ron_comma')
            if pos >= len(tokens): raise ValueError('ron_end')
            pos += 1
            return items if token == '[' else tuple(items)
        if token in ('Some', 'Name'):
            arg = value(depth+1)
            if not isinstance(arg, tuple) or len(arg) != 1: raise ValueError('ron_variant')
            return ('variant', token, arg[0])
        if token in ('None', 'All', 'Active'): return ('variant', token)
        raise ValueError('ron_variant')
    result = value()
    if pos != len(tokens): raise ValueError('ron_trailing')
    return result


def strings(value):
    if not isinstance(value, list) or len(value) > 64 or not all(isinstance(v, str) and ID.fullmatch(v) for v in value):
        raise ValueError('configuration_identifiers')
    return value


def optional(value):
    if value == ('variant', 'None'): return None
    if isinstance(value, tuple) and len(value) == 3 and value[:2] == ('variant', 'Some'): return value[2]
    raise ValueError('configuration_option')


def output_names(env=None):
    """Read wl_output v4 names through a separate bounded registry connection.

    Never consumes WAYLAND_SOCKET, launches a host, or guesses DRM connectors.
    The second sync completes initial output events. Removal/old protocol fails
    closed. Native qualification must verify this protocol/IO scope.
    """
    env = os.environ if env is None else env
    display = env.get('WAYLAND_DISPLAY')
    if not display: raise ValueError('wayland_display_missing')
    path = Path(display) if display.startswith('/') else Path(env['XDG_RUNTIME_DIR']) / display
    deadline = time.monotonic() + 1
    def string(s):
        b = s.encode()+b'\0'; return struct.pack('I', len(b))+b+b'\0'*((-len(b)) % 4)
    def decode(b):
        if len(b) < 4: raise ValueError('wayland_string')
        size, = struct.unpack_from('I', b)
        end = 4+((size+3)//4)*4
        if not 1 <= size <= len(b)-4 or end > len(b) or b[3+size] != 0: raise ValueError('wayland_string')
        if b'\0' in b[4:3+size]: raise ValueError('wayland_string')
        return b[4:3+size].decode(), end
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
        def remaining():
            left = deadline-time.monotonic()
            if left <= 0: raise ValueError('wayland_deadline')
            client.settimeout(left)
        remaining(); client.connect(str(path))
        def send(obj, opcode, payload):
            remaining()
            client.sendall(struct.pack('II', obj, ((len(payload)+8)<<16)|opcode)+payload)
        def receive(n):
            result = b''
            while len(result) < n:
                remaining(); chunk = client.recv(n-len(result))
                if not chunk: raise ValueError('wayland_closed')
                result += chunk
            return result
        # One client namespace: no gaps, no collisions, no callback-ID reuse.
        # The bounded connection allocates at most 35 objects, then disconnects.
        next_id = 2
        def allocate():
            nonlocal next_id
            allocated = next_id; next_id += 1
            return allocated
        registry = allocate(); callback = allocate()
        send(1, 1, struct.pack('I', registry)); send(1, 0, struct.pack('I', callback))
        outputs = {}; first_round = True
        for _ in range(4096):
            obj, header = struct.unpack('II', receive(8)); size, op = header >> 16, header & 65535
            if not 8 <= size <= 65536 or size % 4: raise ValueError('wayland_message')
            body = receive(size-8)
            if obj == 1 and op == 0:
                if len(body) < 12: raise ValueError('wayland_error_size')
                error_object, code = struct.unpack_from('II', body)
                message, used = decode(body[8:])
                if used+8 != len(body): raise ValueError('wayland_error_size')
                # Only fixed protocol diagnostics can enter logs verbatim. A
                # compositor implementation error may contain arbitrary paths.
                safe = (r'(Invalid new_id: [0-9]{1,10}\.|invalid new id [0-9]{1,10}|'
                        r'Unknown opcode [0-9]{1,5} for interface wl_display\.|'
                        r'invalid arguments for wl_(registry|display|output)#[0-9]{1,10}\.(bind|sync|get_registry))')
                if len(message) > 256 or not re.fullmatch(safe, message):
                    raw = message.encode()
                    message = f'[redacted bytes={len(raw)} sha256={hashlib.sha256(raw).hexdigest()}]'
                raise ValueError(f'wayland_error object={error_object} code={code} message={message}')
            if obj == registry:
                if op == 1: raise Changed('wayland_outputs_changed')
                if op != 0 or len(body) < 12: raise ValueError('wayland_registry')
                global_id, = struct.unpack_from('I', body); interface, off = decode(body[4:])
                if len(body) != off+8: raise ValueError('wayland_registry_size')
                version, = struct.unpack_from('I', body, off+4)
                if interface == 'wl_output':
                    if not first_round or version < 4 or len(outputs) >= 32: raise ValueError('wayland_output_scope')
                    output = allocate(); outputs[output] = None
                    send(registry, 0, struct.pack('I', global_id)+string('wl_output')+struct.pack('II', 4, output))
            elif obj in outputs and op == 4:
                if outputs[obj] is not None: raise Changed('wayland_name_changed')
                name, used = decode(body)
                if used != len(body): raise ValueError('wayland_output_name_size')
                outputs[obj] = name
            elif obj == callback and op == 0:
                if len(body) != 4: raise ValueError('wayland_callback')
                if first_round:
                    first_round = False; callback = allocate()
                    send(1, 0, struct.pack('I', callback))
                else:
                    names = list(outputs.values())
                    if not names or any(not isinstance(n,str) or not ID.fullmatch(n) for n in names) or len(set(names)) != len(names):
                        raise ValueError('wayland_output_names')
                    return sorted(names)
    raise ValueError('wayland_event_limit')


class Snapshot:
    def __init__(self, policy=None, env=None):
        self.env = dict(os.environ if env is None else env)
        self.watch = Watch(); self.records = {}; self.policy = policy
        self.applets = {}
        try:
            self.outputs = output_names(self.env)
            if len(self.outputs) > (policy['max_outputs'] if policy is not None else 32): raise ValueError('unqualified_output_count')
            self.counts, bars = self._inventory()
            self.identity = identity(dict(resolver=RESOLVER, native_source=CONFIGURATION_SOURCE, outputs=self.outputs, bars=bars, files=self.records,
                                          counts=self.counts, path=self.env.get('PATH')))
            if self.watch.changed() or output_names(self.env) != self.outputs: raise Changed('configuration_changed')
        except BaseException:
            self.close(); raise

    def close(self): self.watch.close()
    def changed(self):
        if self.watch.changed(): return True
        try: return output_names(self.env) != self.outputs
        except (OSError, ValueError, KeyError): return True

    def read(self, path, *, optional=False):
        path = Path(path); self.watch.add(path, entry=optional, optional=optional)
        if not path.exists(): self.records[str(path)] = None; return None
        if not path.is_file() or path.stat().st_size > 65536: raise ValueError('configuration_file')
        data = path.read_bytes(); self.records[str(path)] = hashlib.sha256(data).hexdigest()
        return data.decode()

    def islands_enabled(self, name):
        # islands.rs::settings_path accepts exactly these two names and does
        # not use XDG data/system defaults for its sidecar.
        if name not in ('Dock', 'Panel'): return False
        config = Path(self.env['XDG_CONFIG_HOME']) if 'XDG_CONFIG_HOME' in self.env else Path(self.env['HOME'])/'.config'
        if not config.is_absolute(): raise ValueError('islands_relative_config_root')
        path = config/'cosmic/io.github.desktopcards.DockIslands/v1'/name
        self.watch.add(path)
        try:
            stat = path.stat()
            if not path.is_file(): raise OSError('not a settings file')
            if stat.st_size > 65536:
                # Native rejects >MAX_WIRE_BYTES; no need to read an unbounded
                # invalid file. Metadata and the watch bind that invalid state.
                self.records[str(path)] = dict(invalid='oversized', size=stat.st_size,
                    mtime=stat.st_mtime_ns, inode=stat.st_ino)
                return False
            data = path.read_bytes()
        except FileNotFoundError:
            self.records[str(path)] = None
            return False  # Settings::default().enabled
        except OSError:
            self.records[str(path)] = dict(invalid='unreadable')
            return False  # load_settings returned Err, no Islands fallback
        self.records[str(path)] = hashlib.sha256(data).hexdigest()
        if len(data) > 65536: return False
        try: text = data.decode('utf-8')
        except UnicodeDecodeError: return False
        return islands_settings_enabled(text)

    def directory(self, path, *, optional=False):
        self.watch.add(path, directory=True, entry=optional, optional=optional)
        if not path.exists(): self.records[str(path)] = None; return False
        if not path.is_dir(): raise ValueError('configuration_directory')
        return True

    def _configured_applet(self, path):
        """Observe the native loader's input; never execute it or gate the host."""
        if path is None: return dict(issue='desktop_entry_missing')
        try:
            fields = desktop(self.read(path, optional=True))
            command = fields.get('Exec')
            if not command: return dict(issue='desktop_exec_missing')
            # The pinned native host uses Rust Shlex, then Command, not a shell.
            argv = shlex.split(command, comments=True, posix=True)
            if not argv or not argv[0] or any('\0' in arg for arg in argv):
                return dict(issue='desktop_exec_invalid')
            executable = Path(argv[0])
            if not executable.is_absolute():
                if '/' in argv[0]: return dict(issue='desktop_exec_unresolved')
                executable = None
                for directory in self.env.get('PATH', os.defpath).split(':'):
                    if not Path(directory).is_absolute():
                        return dict(issue='desktop_exec_unresolved')
                    choice = Path(directory)/argv[0]
                    self.watch.add(choice, entry=True, optional=True)
                    if choice.is_file() and os.access(choice, os.X_OK):
                        executable = choice; break
                if executable is None: return dict(issue='applet_executable_missing')
            self.watch.add(executable, entry=True, optional=True)
            resolved = executable.resolve(strict=True)
            self.watch.add(resolved, optional=True)
            if not resolved.is_file() or not os.access(executable, os.X_OK):
                return dict(issue='applet_executable_missing')
            # Wrappers may replace themselves or delegate outside the host tree.
            # Do not mistake that for a dead applet; the native host owns launching.
            wrapped = 'X-Flatpak' in fields or executable.name in (
                'env', 'flatpak', 'sh', 'bash', 'dash', 'zsh', 'bwrap', 'systemd-run')
            fd = os.open(resolved, os.O_RDONLY | os.O_NONBLOCK | os.O_CLOEXEC)
            try: wrapped |= os.read(fd, 2) == b'#!'
            finally: os.close(fd)
            return dict(executable=str(executable), resolved=str(resolved),
                        args=argv[1:], wrapped=wrapped)
        except FileNotFoundError:
            return dict(issue='applet_executable_missing')
        except (OSError, ValueError, UnicodeError):
            return dict(issue='desktop_entry_invalid')

    def _inventory(self):
        env = self.env
        if env.get('FLATPAK_ID') or env.get('COSMIC_PANEL_CONFIG'): raise ValueError('configuration_environment_unsupported')
        home = Path(env['HOME'])
        config_home = Path(env.get('XDG_CONFIG_HOME') or home/'.config')
        data_home = Path(env.get('XDG_DATA_HOME') or home/'.local/share')
        data_dirs = [data_home]+[Path(p) for p in (env.get('XDG_DATA_DIRS') or '/usr/local/share:/usr/share').split(':') if p.startswith('/')]
        if not home.is_absolute() or not config_home.is_absolute() or not data_home.is_absolute(): raise ValueError('configuration_relative_root')
        system = {}
        def get(name, key, default):
            relative = Path('cosmic')/name/'v1'
            # cosmic-config selects the first existing namespace directory,
            # then falls back per-key only from the user config to that directory.
            if name not in system:
                system[name] = None
                for base in data_dirs:
                    if self.directory(base/relative): system[name] = base/relative; break
            raw = self.read(config_home/relative/key)
            if raw is None and system[name] is not None: raw = self.read(system[name]/key)
            return default if raw is None else ron(raw)
        entries = get(PREFIX, 'entries', None)
        if entries is None:
            # Container default applies only when entries itself is missing.
            # Per-entry missing fields otherwise use CosmicPanelConfig::default.
            builtins = [('Panel', ['com.system76.CosmicPanelWorkspacesButton','com.system76.CosmicPanelAppButton',
                'com.system76.CosmicAppletTime','com.system76.CosmicAppletInputSources','com.system76.CosmicAppletA11y',
                'com.system76.CosmicAppletStatusArea','com.system76.CosmicAppletTiling','com.system76.CosmicAppletAudio',
                'com.system76.CosmicAppletBluetooth','com.system76.CosmicAppletNetwork','com.system76.CosmicAppletBattery',
                'com.system76.CosmicAppletNotifications','com.system76.CosmicAppletPower']),
                ('Dock',['com.system76.CosmicPanelLauncherButton','com.system76.CosmicPanelWorkspacesButton',
                 'com.system76.CosmicPanelAppButton','com.system76.CosmicAppList','com.system76.CosmicAppletMinimize'])]
            bars = [(name, ('variant','All'), plugins) for name,plugins in builtins]
        else:
            entries = strings(entries)
            if len(set(entries)) != len(entries): raise ValueError('duplicate_bar')
            bars = []
            for entry in entries:
                namespace = PREFIX+'.'+entry
                name = get(namespace,'name','')
                if name != entry: raise ValueError('ambiguous_bar_name')
                output = get(namespace,'output',('variant','All'))
                wings = optional(get(namespace,'plugins_wings',('variant','None')))
                center = optional(get(namespace,'plugins_center',('variant','None')))
                if wings is not None and (not isinstance(wings,tuple) or len(wings)!=2): raise ValueError('configuration_wings')
                plugins = (strings(wings[0]) if wings else []) + (strings(center) if center is not None else []) + (strings(wings[1]) if wings else [])
                bars.append((entry, output, plugins))
        counts = {p:0 for p in self.policy['processes']} if self.policy is not None else {}
        wanted = {p for _,_,ps in bars for p in ps}
        if len(wanted) > 256: raise ValueError('configured_applet_limit')
        if self.policy is not None and wanted - self.policy['applets'].keys(): raise ValueError('unqualified_configured_applet')
        # Pinned Iter visits direct files sorted, then queued subdirectories in
        # reverse encountered order, before the next XDG root. file_stem (not
        # flattened desktop ID) selects each plugin's first matching path.
        paths = {}; pending = deque(base/'applications' for base in data_dirs); seen = set(); visited = 0
        while pending and wanted - paths.keys():
            directory = pending.popleft()
            if directory in seen: continue
            seen.add(directory)
            try:
                if not self.directory(directory, optional=self.policy is None): continue
                children = sorted(directory.iterdir())
            except (OSError, ValueError):
                if self.policy is not None: raise
                continue  # Optional applet discovery cannot reject the host.
            for path in children:
                visited += 1
                if visited > 10000: raise ValueError('desktop_inventory_limit')
                if path.is_dir(): pending.appendleft(path)
                elif path.suffix == '.desktop' and path.stem in wanted and path.stem not in paths: paths[path.stem] = path
        if self.policy is None:
            for plugin in sorted(wanted):
                self.applets[plugin] = self._configured_applet(paths.get(plugin))
                counts[plugin] = 0
        elif wanted - paths.keys(): raise ValueError('configured_desktop_missing')
        for plugin,path in (paths.items() if self.policy is not None else ()):
            fields = desktop(self.read(path))
            command = fields.get('Exec')
            if not command or 'X-Flatpak' in fields: raise ValueError('desktop_exec_unsupported')
            if any(c in command for c in ('#', '\\', '\n', '\r', '\0')):
                raise ValueError('desktop_exec_syntax_unsupported')
            argv = shlex.split(command, comments=False, posix=True)
            qualified = self.policy['applets'][plugin]
            if qualified.get('exec_name') and (not argv or Path(argv[0]).name != qualified['exec_name']): raise ValueError('desktop_exec_contract')
            if not argv or argv[1:] != qualified['args']: raise ValueError('desktop_arguments_unqualified')
            exe = Path(argv[0])
            if not exe.is_absolute():
                if '/' in argv[0]: raise ValueError('desktop_relative_exec')
                choices = [Path(p)/argv[0] for p in env.get('PATH',os.defpath).split(':')]
                if any(not p.is_absolute() for p in choices): raise ValueError('desktop_relative_path')
                exe = None
                for choice in choices:
                    if choice.is_file() and os.access(choice, os.X_OK): exe = choice; break
                    self.watch.add(choice)
                if exe is None: raise ValueError('desktop_exec_missing')
            # Executable bytes are verified by installed/runtime IO, not this
            # config watcher. Replacing code remains a code failure, not a race
            # excuse suppressing the crash latch.
            if str(exe.resolve()) != self.policy['processes'][qualified['process']]: raise ValueError('desktop_exec_unqualified')
        normalized = []
        for name,output,plugins in bars:
            enabled = self.islands_enabled(name)
            selected = None
            if self.policy is not None and len(set(plugins)) != len(plugins): raise ValueError('duplicate_plugin_on_bar_unsupported')
            if output == ('variant','All'): multiplier = len(self.outputs)
            elif output == ('variant','Active'): multiplier = 1
            elif isinstance(output,tuple) and len(output)==3 and output[:2]==('variant','Name') and isinstance(output[2],str):
                selected = output[2] if output[2] in self.outputs else (
                    min(self.outputs, key=lambda n: (not n.startswith(('eDP','LVDS','DSI')), n)) if enabled and self.outputs else None)
                multiplier = int(selected is not None)
            else: raise ValueError('configuration_output')
            for plugin in plugins:
                process = self.policy['applets'][plugin]['process'] if self.policy is not None else plugin
                counts[process] += multiplier
                if self.policy is not None and counts[process] > 64: raise ValueError('configured_count_limit')
            normalized.append(dict(name=name,output=output,plugins=plugins,multiplier=multiplier,
                                   islands_enabled=enabled,selected_output=selected))
        return counts, normalized


class UnsupportedSettings(ValueError):
    """Outside the bounded parser scope: reject admission, never guess disabled."""


def islands_settings_enabled(text):
    """Deserialize/validate Settings' canonical RON subset, as the patched core.

    Known malformed/schema-invalid input returns False like load_settings Err.
    RON extensions outside this reader's scope reject the snapshot, so a valid
    enabled extension can never accidentally be counted as disabled.
    """
    class Struct(dict): pass
    class Map(dict): pass
    tokens = []; i = 0
    try:
        while i < len(text):
            if text[i].isspace(): i += 1; continue
            if text.startswith('//', i):
                end=text.find('\n',i); i=len(text) if end<0 else end+1; continue
            if text.startswith('/*', i):
                depth=1; i+=2
                while i<len(text) and depth:
                    if text.startswith('/*',i): depth+=1; i+=2
                    elif text.startswith('*/',i): depth-=1; i+=2
                    else:i+=1
                if depth: return False
                continue
            if text[i]=='"':
                literal=re.match(r'"(?:\\.|[^"\\])*"',text[i:],re.S)
                if literal and any(e not in ('\\','"','n','r','t') for e in re.findall(r'\\(.)',literal[0],re.S)):
                    raise UnsupportedSettings('islands_ron_string_extension')
                value,length=json.JSONDecoder().raw_decode(text[i:])
                if any(e not in ('\\','"','n','r','t') for e in re.findall(r'\\(.)',text[i:i+length])):
                    raise UnsupportedSettings('islands_ron_string_extension')
                if any(0xD800<=ord(c)<=0xDFFF for c in value):return False
                tokens.append(('string',value));i+=length;continue
            match=re.match(r'[A-Za-z_][A-Za-z_0-9]*|[+-]?[0-9]+|[{}\[\]():,]',text[i:])
            if not match:raise UnsupportedSettings('islands_ron_extension')
            tokens.append(match[0]);i+=len(match[0])
        at=0
        def value(depth=0):
            nonlocal at
            if depth>16 or at>=len(tokens):raise ValueError('islands_ron_end')
            token=tokens[at];at+=1
            if isinstance(token,tuple):return token[1]
            if token in ('(', '{', '['):
                end={'(':')','{':'}','[':']'}[token]
                # Serde fixed arrays use RON tuples, not sequences. Distinguish
                # (field:value) structs from (value,value,...) before reading;
                # keep tuple/list shape so native-invalid lists stay invalid.
                is_tuple=token=='(' and at<len(tokens) and tokens[at]!=')' and not (
                    at+1<len(tokens) and tokens[at+1]==':')
                sequence=token=='[' or is_tuple
                result=[] if sequence else Struct() if token=='(' else Map()
                while at<len(tokens) and tokens[at]!=end:
                    if sequence:result.append(value(depth+1))
                    else:
                        key=tokens[at];at+=1
                        if token=='{':
                            if not isinstance(key,tuple):raise UnsupportedSettings('islands_map_key_extension')
                            key=key[1]
                        elif not isinstance(key,str) or not re.fullmatch('[A-Za-z_][A-Za-z_0-9]*',key):
                            raise UnsupportedSettings('islands_tuple_extension')
                        if at>=len(tokens) or tokens[at]!=':':raise ValueError('islands_ron_colon')
                        at+=1
                        if token=='(' and key in result:raise ValueError('islands_duplicate_field')
                        result[key]=value(depth+1)
                    if at<len(tokens) and tokens[at]==',':at+=1
                    elif at>=len(tokens) or tokens[at]!=end:raise ValueError('islands_ron_comma')
                if at>=len(tokens):raise ValueError('islands_ron_end')
                at+=1;return tuple(result) if is_tuple else result
            if at<len(tokens) and tokens[at]=='(':raise UnsupportedSettings('islands_named_or_tuple_extension')
            if token in ('true','false'):return token=='true'
            if re.fullmatch('[+-]?[0-9]+',token):
                if at<len(tokens) and isinstance(tokens[at],str) and re.match('[A-Za-z_]',tokens[at]):
                    raise UnsupportedSettings('islands_number_extension')
                return int(token)
            if re.fullmatch('[A-Za-z_][A-Za-z_0-9]*',token):return ('enum',token)
            raise ValueError('islands_ron_token')
        settings=value()
        if at!=len(tokens):return False
        def fields(record, names):
            if type(record) is not Struct or set(record)!=set(names):raise ValueError('islands_fields')
        def uint(n,limit):
            if type(n) is not int or not 0<=n<=limit:raise ValueError('islands_number')
        def order(items):
            if type(items) is not tuple or len(items)!=3 or set(items)!={('enum','Start'),('enum','Center'),('enum','End')}:
                raise ValueError('islands_order')
        def arrangement(layout):
            fields(layout,('together','separate'))
            together,separate=layout['together'],layout['separate']
            fields(together,('order','position'));fields(separate,('order','positions'))
            order(together['order']);order(separate['order']);uint(together['position'],10000)
            if type(separate['positions']) is not tuple or len(separate['positions'])!=3:raise ValueError('islands_positions')
            for n in separate['positions']:uint(n,10000)
        fields(settings,('version','enabled','placement','gap','default_layout','outputs'))
        if type(settings['version']) is not int or settings['version']!=1 or type(settings['enabled']) is not bool: return False
        if settings['placement'] not in (('enum','Together'),('enum','Separate')):return False
        uint(settings['gap'],64);arrangement(settings['default_layout'])
        outputs=settings['outputs']
        if type(outputs) is not Map or len(outputs)>32:return False
        for name,layout in outputs.items():
            if not isinstance(name,str) or not 1<=len(name.encode('utf-8'))<=128 or any(ord(c)<32 or 127<=ord(c)<=159 for c in name):return False
            arrangement(layout)
        return settings['enabled']
    except UnsupportedSettings:
        raise
    except (ValueError,TypeError,KeyError):
        return False


def desktop(text):
    if text is None: raise ValueError('desktop_missing')
    groups = {}; group = None
    for line in text.splitlines():
        if not line.strip() or line.startswith('#'): continue
        if line.startswith('[') and line.endswith(']'):
            group = line[1:-1]
            if group in groups: raise ValueError('desktop_duplicate_group')
            groups[group] = {}; continue
        if group is None or '=' not in line: raise ValueError('desktop_line')
        key,value = line.split('=',1); key=key.strip()
        if not key: raise ValueError('desktop_key')
        # Reject duplicate keys and localized Exec (not a portable launch
        # contract); the supported direct Exec is unambiguous across locales.
        if key in groups[group] or key.startswith('Exec['): raise ValueError('desktop_ambiguous_key')
        escapes={'s':' ','n':'\n','t':'\t','r':'\r','\\':'\\'}
        def unescape(match):
            if match[1] not in escapes: raise ValueError('desktop_escape')
            return escapes[match[1]]
        if value.endswith('\\') and len(value)-len(value.rstrip('\\')) & 1: raise ValueError('desktop_trailing_escape')
        if value.startswith(' '): value=value[1:]
        groups[group][key] = re.sub(r'\\(.)',unescape,value)
    if 'Desktop Entry' not in groups: raise ValueError('desktop_group')
    return groups['Desktop Entry']


def policy(profile):
    config = profile['configuration']
    return {**config, 'processes': {n:p['executable'] for n,p in profile['processes'].items() if p['lifecycle']=='panel-child'}}
