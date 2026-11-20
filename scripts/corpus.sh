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


def canonical_macros(text):
    # Bracing a reference is an intentional formatting change, not content loss.
    out = []
    i = 0
    while i < len(text):
        if text.startswith("%%", i):
            out.append("%%")
            i += 2
            continue
        match = re.match(r'%\{([!?]*[A-Za-z_][A-Za-z0-9_]*)\}', text[i:])
        if match:
            out.append("%" + match.group(1))
            i += len(match.group(0))
        else:
            out.append(text[i])
            i += 1
    return "".join(out)


def dependency_tokens(value):
    # Ignore only top-level commas, which separate dependency expressions.
    depth = 0
    chars = []
    for char in value:
        if char in "({":
            depth += 1
        elif char in ")}":
            depth -= 1
        chars.append(" " if char == "," and depth == 0 else char)
    return "".join(chars).split()


def records(source):
    out = collections.Counter()
    for line in source.splitlines():
        line = canonical_macros(line)
        definition = re.match(r'\s*%(?:define|global)\s+([A-Za-z0-9_]+)\s+([^%\\]+)$', line)
        if definition:
            line = f"%global {definition.group(1)} {definition.group(2)}"
        match = TAG_RE.match(line)
        if match:
            tag, value = match.group(1), match.group(2).strip()
            if tag.split("(")[0] in DEP:
                for token in dependency_tokens(value):
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
