import io

path = r"client/src/level/parse.rs"
with io.open(path, "r", encoding="utf-8") as f:
    lines = f.readlines()

# Print lines 400..470 with explicit repr so we see exact characters.
for i in range(400, min(len(lines), 470)):
    print("%d|%s" % (i + 1, repr(lines[i])))