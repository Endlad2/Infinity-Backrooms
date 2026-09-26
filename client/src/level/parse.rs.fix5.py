import io, re

path = r"client/src/level/parse.rs"
with io.open(path, "r", encoding="utf-8") as f:
    lines = f.readlines()

# Replace line 367 (0-indexed 366) with a clean line that has no backslashes.
# We write it in a way that does not introduce any \n escape at all.
new_line = '    <model id="mdl_b"><obj><obj>o cube_n 0 0 0</obj></obj></model>\n'

if 366 < len(lines):
    lines[366] = new_line

with io.open(path, "w", encoding="utf-8") as f:
    f.writelines(lines)

print("fix5 rewrote line 367")