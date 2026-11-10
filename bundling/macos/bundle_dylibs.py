#!/usr/bin/env python3
# bundle_dylibs.py <exe> <frameworks_dir>
# Discovers all non-system Homebrew dylibs the binary (transitively) depends on,
# copies them into frameworks_dir, and rewrites all load commands to use
# @executable_path/../Frameworks (for the exe) or @loader_path (for dylibs).
# Exits non-zero if any bundled object still depends on something outside the app.

import os
import shutil
import subprocess
import sys
from collections import deque

SYSTEM_PREFIXES = ("/usr/lib/", "/System/Library/")


def otool_deps(path, skip_id=False):
    out = subprocess.check_output(["otool", "-L", path], text=True)
    lines = out.splitlines()[1:]  # drop the object name line
    if skip_id and lines:
        lines = lines[1:]  # first entry is the dylib's own install name
    return [dep for line in lines if (dep := line.strip().split(" (")[0])]


def rpaths(path):
    lines = [line.strip() for line in subprocess.check_output(["otool", "-l", path], text=True).splitlines()]
    return [
        follow[len("path "):].split(" (offset")[0]
        for i, line in enumerate(lines)
        if line == "cmd LC_RPATH"
        for follow in lines[i + 1:i + 3]
        if follow.startswith("path ")
    ]


def resolve_dep(dep, src):
    # src is where the referencing object came from, so @loader_path and
    # @rpath expand against its original directory, not the copy in the bundle
    base = os.path.dirname(src)

    def expand(path):
        for token in ("@loader_path", "@executable_path"):
            if path.startswith(token):
                return base + path[len(token):]
        return path

    if dep.startswith("@rpath/"):
        rest = dep[len("@rpath/"):]
        candidates = [os.path.join(expand(r), rest) for r in rpaths(src)]
    elif dep.startswith(("@loader_path/", "@executable_path/")):
        candidates = [expand(dep)]
    elif dep.startswith("/"):
        candidates = [dep]
    else:
        candidates = []
    return next((c for c in candidates if os.path.isfile(c)), None)


def is_system(dep):
    return dep.startswith(SYSTEM_PREFIXES)


def bundle(exe, frameworks_dir):
    os.makedirs(frameworks_dir, exist_ok=True)

    source = {exe: exe}  # bundled path -> original path
    queue = deque([exe])
    while queue:
        cur = queue.popleft()
        for dep in otool_deps(cur, skip_id=cur != exe):
            if is_system(dep) or (resolved := resolve_dep(dep, source[cur])) is None:
                continue
            dst = os.path.join(frameworks_dir, os.path.basename(dep))
            if dst not in source:
                shutil.copy2(resolved, dst)
                os.chmod(dst, os.stat(dst).st_mode | 0o200)
                source[dst] = resolved
                queue.append(dst)

    for obj in source:
        prefix = "@executable_path/../Frameworks" if obj == exe else "@loader_path"
        for dep in otool_deps(obj, skip_id=obj != exe):
            name = os.path.basename(dep)
            new_dep = f"{prefix}/{name}"
            if not is_system(dep) and dep != new_dep and os.path.join(frameworks_dir, name) in source:
                subprocess.check_call(["install_name_tool", "-change", dep, new_dep, obj])

    verify(exe, frameworks_dir, source)


def verify(exe, frameworks_dir, bundled):
    broken = []
    for obj in bundled:
        prefix = "@executable_path/../Frameworks/" if obj == exe else "@loader_path/"
        for dep in otool_deps(obj, skip_id=obj != exe):
            if is_system(dep):
                continue
            name = dep.removeprefix(prefix)
            if name == dep or not os.path.isfile(os.path.join(frameworks_dir, name)):
                broken.append(f"{os.path.basename(obj)} -> {dep}")
    if broken:
        sys.exit("[error] dylib dependencies left outside the bundle:\n  " + "\n  ".join(broken))
    print(f"[ok] bundled {len(bundled) - 1} dylibs")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print(f"usage: {sys.argv[0]} <exe> <frameworks_dir>", file=sys.stderr)
        sys.exit(1)
    bundle(sys.argv[1], sys.argv[2])
