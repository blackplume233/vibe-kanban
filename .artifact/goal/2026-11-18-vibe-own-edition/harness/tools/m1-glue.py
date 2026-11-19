"""Allow NSH's page host to supply the wasm-bindgen runtime's NSH imports."""

from pathlib import Path
import re
import sys

path = Path(sys.argv[1])
source = path.read_text(encoding="utf-8")
for function in ("__wbg_get_imports", "__wbg_finalize_init"):
    if f"function {function}(" not in source:
        raise ValueError(f"wasm-bindgen glue missing {function}")
source, replacements = re.subn(
    r'^import \* as (\w+) from [\'\"]nsh[\'\"];?$', r"const \1 = {};", source, flags=re.MULTILINE
)
if not replacements:
    raise ValueError("wasm-bindgen glue has no NSH imports")
path.write_text(
    source + "\nexport { __wbg_get_imports as __nsh_imports, __wbg_finalize_init as __nsh_finalize };\n",
    encoding="utf-8",
    newline="\n",
)
