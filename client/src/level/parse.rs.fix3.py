import io, re

path = r"client/src/level/parse.rs"
with io.open(path, "r", encoding="utf-8") as f:
    src = f.read()

# Strip any BOM/odd chars at start
src = src.replace("\r\n", "\n").replace("\r", "\n")

# Normalize the notlevel test — write it in an absolutely unambiguous way.
# Find the whole rejects_wrong_root function body and replace it.
pat_fn = re.compile(
    r"fn\s+rejects_wrong_root\s*\(\s*\)\s*\{.*?\n\s*\}",
    re.DOTALL,
)
replacement = (
    "fn rejects_wrong_root() {\n"
    "        let bad = String::from(\"<notlevel>\") + \"/>\";\n"
    "        let r = parse_level_xml(\u0026bad);\n"
    "        assert!(r.is_err());\n"
    "    }"
)
new_src, n = pat_fn.subn(replacement, src, count=1)

if n == 0:
    # Fallback: just replace the literal line.
    new_src = src.replace(
        '        let r = parse_level_xml(r#"<notlevel/>"#);',
        '        let bad = String::from("<notlevel>") + "/>";\n        let r = parse_level_xml(&bad);',
    )

with io.open(path, "w", encoding="utf-8") as f:
    f.write(new_src)

print("fix3 patched, fn_replacements=%d" % n)