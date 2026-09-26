import io, re, sys

path = r"client/src/level/parse.rs"
with io.open(path, "r", encoding="utf-8") as f:
    src = f.read()

# Fix 1: replace literal backslash-n inside a Rust raw-ish string test on line 367.
bad1 = 'o cube\\nv 0 0 0'
good1 = 'o cube' + chr(92) + 'n' + 'v 0 0 0'  # placeholder, will be overwritten below
# Actually: we want the Rust source to contain a real backslash-n escape, i.e. two chars \ and n.
# In Python string, that's "\\n" (backslash + n). We need to write it into the file as-is.
target_old = 'o cube\\nv 0 0 0'   # backslash + n (two chars) as it should be in raw file
# But the file currently has a *raw* backslash followed by 'n' too (the compiler complained).
# The compiler complained "unknown start of token: \" which means the string was NOT a raw string,
# so \n was treated as escape sequence start and then 'v' broke it. The fix is to make the string
# a raw string r"...".
# Simplest: replace the whole line with a raw-string version.
old_line = '    <model id="mdl_b"><obj><obj>o cube\\nv 0 0 0</obj></obj></model>'
new_line = '    r#"<model id="mdl_b"><obj><obj>o cube\\nv 0 0 0</obj></obj></model>"#'
if old_line in src:
    src = src.replace(old_line, new_line, 1)

# Fix 2: the notlevel test — ensure the string literal is properly closed.
# The compiler shows:
#   let r = parse_level_xml("<notlevel/>");
# but inside a raw string with a stray line-continuation, this may be inside a broken raw string
# because of fix 1's fallout. Once fix 1 is applied the rest should compile.
# To be safe, ensure the exact snippet exists in a normal form:
bad2 = 'let r = parse_level_xml("<notlevel/>");'
good2 = 'let r = parse_level_xml("<notlevel/>");'
if bad2 not in src:
    # nothing to do; the original code is fine once fix 1 is applied
    pass

with io.open(path, "w", encoding="utf-8", newline="\n") as f:
    f.write(src)

print("patched")