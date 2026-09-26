import io, re

path = r"client/src/level/parse.rs"
with io.open(path, "r", encoding="utf-8", newline="") as f:
    src = f.read()

# Rewrite the parse.rs file entirely, replacing any malformed notlevel test line with a safe form.
# Find and replace the specific call in rejects_wrong_root.
bad1 = '        let r = parse_level_xml("<notlevel/>");'
good1 = '        let r = parse_level_xml(r#"<notlevel/>"#);'
count = 0
if bad1 in src:
    src = src.replace(bad1, good1)
    count += 1

# Also catch any variant with different indent
pat = re.compile(r'(let\s+r\s*=\s*parse_level_xml\(\s*)"(<notlevel\s*/>)"(\s*\)\s*;)')
def _sub(m):
    return m.group(1) + 'r#' + m.group(2) + '#' + m.group(3)
new_src, n2 = pat.subn(_sub, src)
if n2:
    src = new_src
    count += n2

# Additionally, scan all lines for suspicious unclosed double-quoted string immediately followed by )
# If any line has odd number of double quotes and does NOT look like it continues on the next line, wrap it in a raw string.
out_lines = []
for ln in src.split("\n"):
    stripped = ln.strip()
    if stripped.startswith("let") and 'parse_level_xml("' in stripped and not stripped.endswith(');'):
        # already too broken; skip
        pass
    out_lines.append(ln)
src = "\n".join(out_lines)

with io.open(path, "w", encoding="utf-8", newline="") as f:
    f.write(src)

print("fix2 patched, replacements=%d" % count)