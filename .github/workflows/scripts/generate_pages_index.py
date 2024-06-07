#!python3
# Generate index.html for code coverage pages

from glob import iglob
from base64 import urlsafe_b64encode, urlsafe_b64decode

master_path = urlsafe_b64encode(b"master").decode("utf-8")

print("<html><head><title>Code Coverage</title>")
print("<style>body { font-size: 1.2em }</style>")
print(f"</head><body><h1>Code coverage</h1>")
print(f"<h2>Per branch (<a href=\"{master_path}/index.html\">master</a>)</h2><ul>")
for path in sorted(list(iglob("*/index.html", recursive=True))):
    name = urlsafe_b64decode(path.split("/")[0].encode()).decode()
    style = "font-weight: bold" if name == "master" else ""
    print(f'<li style="{style}"><a href="{path}">{name}</a></li>')
print("</ul></body></html>")
