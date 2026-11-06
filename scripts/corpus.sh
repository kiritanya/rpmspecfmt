#!/bin/sh
set -eu

root=${1:-../terra}
bin=${BIN:-target/debug/rpmspecfmt}

if [ ! -x "$bin" ]; then
    cargo build
fi

python3 - "$root" "$bin" <<'PY'
import collections
import glob
import os
import re
import subprocess
import sys

root, binary = sys.argv[1], sys.argv[2]

TAG_RE = re.compile(r'^([A-Za-z][A-Za-z0-9_]*(?:\([^)]*\))?):[ \t]*(.*)$')
DEP = {
    "BuildRequires", "Requires", "Provides", "Conflicts", "Obsoletes",
    "Recommends", "Suggests", "Supplements", "Enhances", "BuildConflicts",
    "OrderWithRequires",
}


def records(source):
    out = collections.Counter()
    for line in source.splitlines():
        match = TAG_RE.match(line)
        if match:
            tag, value = match.group(1), match.group(2).strip()
            if tag.split("(")[0] in DEP:
                for token in value.split():
                    out[("dep", tag, token)] += 1
            else:
                out[("tag", tag, value)] += 1
        else:
            text = line.strip()
            if text:
                out[("line", text)] += 1
    return out


def run(source):
    return subprocess.run(
        [binary],
        input=source.encode(),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )


total = changed = 0
errors = []
for path in sorted(glob.glob(os.path.join(root, "**", "*.spec"), recursive=True)):
    total += 1
    source = open(path, encoding="utf-8", errors="replace").read()
    first = run(source)
    if first.returncode != 0:
        errors.append((path, first.stderr.decode().strip()))
        continue
    formatted = first.stdout.decode()
    if formatted != source:
        changed += 1

    if records(formatted) != records(source):
        errors.append((path, "content changed"))
        continue

    second = run(formatted)
    if second.stdout.decode() != formatted:
        errors.append((path, "not idempotent"))

print(f"specs={total} changed={changed} failures={len(errors)}")
for path, message in errors[:20]:
    print(f"  {path}: {message}")
sys.exit(1 if errors else 0)
PY
