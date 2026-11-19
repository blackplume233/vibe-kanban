#!/usr/bin/env python3
"""Create the release/NPX archives from a Cargo build and NSH packages."""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
from zipfile import ZIP_DEFLATED, ZipFile


def web_files(web):
    if not (web / "index.html").is_file() or not any(web.glob("nsh.*.wasm")):
        raise ValueError(f"Expected the locked NSH Web package directory: {web}")
    return [(file, f"web/{file.relative_to(web).as_posix()}")
            for file in sorted(web.rglob("*")) if file.is_file()]


def archive(out, files):
    out.parent.mkdir(parents=True, exist_ok=True)
    with ZipFile(out, "w", compression=ZIP_DEFLATED) as bundle:
        for file, name in files:
            bundle.write(file, name)


def platform_archives(args):
    ext = ".exe" if args.platform.startswith("windows-") else ""
    prefix = "" if args.local else f"{args.platform}-"
    for name, source in [("vibe-kanban", "server"), ("vibe-kanban-mcp", "vibe-kanban-mcp"),
                         ("vibe-kanban-review", "review")]:
        file = args.bins / f"{source if args.cargo_names else name}{ext}"
        files = [(file, name + ext)]
        if name == "vibe-kanban":
            files += web_files(args.web)
        archive(args.out / f"{prefix}{name}.zip", files)


def native_archive(args):
    with ZipFile(args.package) as bundle:
        names = set(bundle.namelist())
        if not {"vibe-own/vibe-own.exe", "vibe-own/nsh/app.json"} <= names:
            raise ValueError(f"Expected the locked NSH Windows ZIP: {args.package}")
    args.out.mkdir(parents=True, exist_ok=True)
    prefix = "" if args.local else "windows-x64-"
    shutil.copyfile(args.package, args.out / f"{prefix}vibe-own.zip")


def manifest(args):
    platforms = {}
    for file in sorted(args.out.glob("*.zip")):
        parts = file.stem.split("-", 2)
        if len(parts) != 3:
            continue
        os, arch, name = parts
        if os not in {"linux", "macos", "windows"} or arch not in {"x64", "arm64"}:
            continue
        with file.open("rb") as data:
            digest = hashlib.file_digest(data, "sha256").hexdigest()
        platforms.setdefault(f"{os}-{arch}", {})[name] = {"sha256": digest, "size": file.stat().st_size}
    (args.out / "manifest.json").write_text(
        json.dumps({"version": args.tag, "platforms": platforms}, indent=2) + "\n", encoding="utf-8"
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(required=True)
    platform = commands.add_parser("platform")
    platform.add_argument("platform")
    platform.add_argument("bins", type=Path)
    platform.add_argument("web", type=Path)
    platform.add_argument("out", type=Path)
    platform.add_argument("--local", action="store_true")
    platform.add_argument("--cargo-names", action="store_true")
    platform.set_defaults(run=platform_archives)
    native = commands.add_parser("native")
    native.add_argument("package", type=Path)
    native.add_argument("out", type=Path)
    native.add_argument("--local", action="store_true")
    native.set_defaults(run=native_archive)
    web = commands.add_parser("web")
    web.add_argument("web", type=Path)
    web.add_argument("out", type=Path)
    web.set_defaults(run=lambda args: archive(args.out, web_files(args.web)))
    index = commands.add_parser("manifest")
    index.add_argument("out", type=Path)
    index.add_argument("tag")
    index.set_defaults(run=manifest)
    args = parser.parse_args()
    args.run(args)


if __name__ == "__main__":
    main()
