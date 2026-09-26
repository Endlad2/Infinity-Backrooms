import io, re

path = r"client/src/level/parse.rs"
with io.open(path, "r", encoding="utf-8") as f:
    src = f.read()

# Line 359 opens a raw string r#"..."#. Line 367 accidentally contains a NESTED raw-string
# marker r#"..."#, which prematurely terminates the outer raw string.
# We must replace the offending line 367 content:
#   r#"<model id="mdl_b"><obj><obj>o cube\\nv 0 0 0</obj></obj></model>"#
# with a plain raw-string-free version that is still valid inside the outer raw string.
old367 = 'r#"<model id="mdl_b"><obj><obj>o cube\\\\nv 0 0 0</obj></obj></model>"#'
new367 = '<model id="mdl_b"><obj><obj>o cube\\\\nv 0 0 0</obj></obj></model>'

if old367 in src:
    src = src.replace(old367, new367)
    print("replaced raw-marker on line 367")
else:
    # Try a simpler match without relying on backslash count
    pat = re.compile(r'r#"<model id="mdl_b">.*?</model>"#', re.DOTALL)
    src, n = pat.subn('<model id="mdl_b"><obj><obj>o cube\\\\nv 0 0 0</obj></obj></model>', src, count=1)
    print("regex replaced: %d" % n)

with io.open(path, "w", encoding="utf-8") as f:
    f.write(src)