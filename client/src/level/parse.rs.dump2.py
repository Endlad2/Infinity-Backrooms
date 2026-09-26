import io

path = r"client/src/level/parse.rs"
with io.open(path, "r", encoding="utf-8") as f:
    lines = f.readlines()

for n in [359, 366, 367, 395, 454, 455, 456, 457, 458]:
    i = n - 1
    if i < len(lines):
        print("%d|%s" % (n, repr(lines[i])))