import io

path = r"client/src/level/parse.rs"
with io.open(path, "r", encoding="utf-8") as f:
    text = f.read()

# Use a wider raw-string delimiter so embedded "# (e.g. color="#ff") does not terminate it.
text = text.replace('const XML: &str = r#"<?xml', 'const XML: &str = r##"<?xml', 1)
# Replace the closing "#; that immediately follows </level>
text = text.replace('</level>"#;', '</level>"##;', 1)

with io.open(path, "w", encoding="utf-8") as f:
    f.write(text)

print("fix6 applied")