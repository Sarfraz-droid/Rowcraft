#!/usr/bin/env python3
"""Freeze resolved JDBC classpaths for the desktop downloader (run after download-drivers.py --all)."""
import hashlib
import json
from pathlib import Path
import runpy

installer = runpy.run_path(str(Path(__file__).with_name("download-drivers.py")))
root = installer["ROOT"]
cache = root / "target/jdbc-maven-cache"
artifacts = {}
for jar in cache.rglob("*.jar"):
    digest = hashlib.sha256(jar.read_bytes()).hexdigest()
    artifacts[(jar.name, digest)] = jar.relative_to(cache).as_posix()
packages = {}
for driver in json.loads(installer["CATALOG"].read_text())["drivers"]:
    if driver["distribution"] != "maven":
        continue
    receipt = json.loads((installer["driver_directory"]() / driver["id"] / "download-receipt.json").read_text())
    if receipt["version"] != driver["version"]:
        raise RuntimeError(f'Stale installed package: {driver["id"]}')
    packages[driver["id"]] = {
        "version": driver["version"],
        "files": [{"path": artifacts[(name, digest)], "sha256": digest,
                   "size": (cache / artifacts[(name, digest)]).stat().st_size}
                  for name, digest in sorted(receipt["files"].items())],
    }
output = root / "drivers/downloads.lock.json"
output.write_text(json.dumps(packages, indent=2, sort_keys=True) + "\n")
print(f"Locked {len(packages)} JDBC packages in {output}")
