import io

path = r"client/src/level/parse.rs"
with io.open(path, "r", encoding="utf-8") as f:
    lines = f.readlines()

n = len(lines)
start = max(0, n - 40)
for i in range(start, n):
    # strip trailing newline
    print("%d: %s" % (i + 1, lines[i].rstrip("\n")))