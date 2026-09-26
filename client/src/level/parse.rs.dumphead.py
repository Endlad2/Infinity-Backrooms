import io

path = r"client/src/level/parse.rs"
with io.open(path, "r", encoding="utf-8") as f:
    lines = f.readlines()

# Print lines 300..400 with repr() to find the unterminated string.
for i in range(300, min(len(lines), 400)):
    print("%d|%s" % (i + 1, repr(lines[i])))