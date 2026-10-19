#!/usr/bin/env python3
"""Install pinned upstream JDBC packages and runtime dependencies; never inspect other apps."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import xml.etree.ElementTree as ET
import zipfile

ROOT = Path(__file__).resolve().parents[1]
CATALOG = ROOT / "drivers/catalog.json"


def driver_directory():
    if os.environ.get("ROWCRAFT_CONFIG_DIR"):
        base = Path(os.environ["ROWCRAFT_CONFIG_DIR"])
    elif os.environ.get("XDG_CONFIG_HOME"):
        base = Path(os.environ["XDG_CONFIG_HOME"]) / "rowcraft"
    elif os.name == "nt":
        base = Path(os.environ.get("APPDATA", "")) / "rowcraft"
    else:
        base = Path.home() / ".config/rowcraft"
    return base / "jdbc"


def install(driver, destination, cache):
    destination.mkdir(parents=True, exist_ok=True)
    # Stage each complete classpath before publishing it. Reinstalls replace the old
    # package so different driver releases cannot accidentally share a classpath.
    with tempfile.TemporaryDirectory(prefix=".download-", dir=destination) as temporary:
        staging = Path(temporary)
        project = ET.Element("project")
        for name, value in (("modelVersion", "4.0.0"), ("groupId", "dev.rowcraft"),
                            ("artifactId", "driver-download"), ("version", "1")):
            ET.SubElement(project, name).text = value
        dependency = ET.SubElement(ET.SubElement(project, "dependencies"), "dependency")
        for name, value in (("groupId", driver["group"]), ("artifactId", driver["artifact"]),
                            ("version", driver["version"])):
            ET.SubElement(dependency, name).text = value
        pom = staging / "pom.xml"
        ET.ElementTree(project).write(pom, encoding="unicode")
        # Resolve only from Central, including repositories declared by dependencies.
        settings = staging / "settings.xml"
        settings.write_text('<settings><mirrors><mirror><id>rowcraft-central</id>'
                            '<mirrorOf>*</mirrorOf><url>https://repo.maven.apache.org/maven2</url>'
                            '</mirror></mirrors></settings>')
        libraries = staging / "libraries"
        subprocess.run([
            "mvn", "-B", "-q", "-C", "-s", str(settings), "-f", str(pom),
            f"-Dmaven.repo.local={cache}",
            "org.apache.maven.plugins:maven-dependency-plugin:3.8.1:copy-dependencies",
            "-DincludeScope=runtime", f"-DoutputDirectory={libraries}",
        ], check=True, timeout=600)
        jars = sorted(libraries.glob("*.jar"))
        if not jars:
            raise RuntimeError("Upstream resolution produced no driver files")
        providers, classes = set(), set()
        for jar in jars:
            try:
                with zipfile.ZipFile(jar) as archive:
                    classes.update(archive.namelist())
                    try:
                        descriptor = archive.read("META-INF/services/java.sql.Driver").decode("utf-8")
                    except KeyError:
                        continue
                    providers.update(line.split("#", 1)[0].strip() for line in descriptor.splitlines())
            except (zipfile.BadZipFile, UnicodeError) as error:
                raise RuntimeError(f"Invalid upstream JAR: {jar.name}: {error}") from error
        if not any(provider and provider.replace(".", "/") + ".class" in classes for provider in providers):
            raise RuntimeError("Package has no loadable JDBC service; it may be a placeholder artifact")
        receipt = {
            "driver": driver["id"], "version": driver["version"], "source": driver["source"],
            "files": {jar.name: hashlib.sha256(jar.read_bytes()).hexdigest() for jar in jars},
        }
        (libraries / "download-receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
        target = destination / driver["id"]
        backup = destination / ("." + driver["id"] + ".previous")
        if backup.exists():
            raise RuntimeError(f"Previous interrupted install needs inspection: {backup}")
        if target.exists():
            target.rename(backup)
        try:
            libraries.rename(target)
        except OSError:
            if backup.exists():
                backup.rename(target)
            raise
        if backup.exists():
            shutil.rmtree(backup)
        return len(jars)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("drivers", nargs="*", help="Database IDs from drivers/catalog.json")
    parser.add_argument("--all", action="store_true", help="Install every Maven JDBC package")
    parser.add_argument("--list", action="store_true", help="List native, Maven and vendor packages")
    parser.add_argument("--directory", type=Path, default=driver_directory())
    parser.add_argument("--cache", type=Path, default=ROOT / "target/jdbc-maven-cache")
    args = parser.parse_args()
    catalog = json.loads(CATALOG.read_text())["drivers"]
    unknown = set(args.drivers) - {driver["id"] for driver in catalog}
    if unknown:
        parser.error("Unknown database IDs: " + ", ".join(sorted(unknown)))
    if args.list:
        for driver in catalog:
            print(f'{driver["id"]:20} {driver["distribution"]:8} {driver.get("version", "")}')
        return 0
    if not args.all and not args.drivers:
        parser.error("Select database IDs, --all, or --list")
    if not shutil.which("mvn"):
        parser.error("Install Apache Maven and a standalone JDK to download JDBC dependencies")
    failures = []
    destination, cache = args.directory.resolve(), args.cache.resolve()
    for driver in catalog:
        if not args.all and driver["id"] not in args.drivers:
            continue
        if driver["distribution"] == "native":
            print(f'{driver["id"]}: native connector; no JDBC download needed', flush=True)
        elif driver["distribution"] == "vendor":
            print(f'{driver["id"]}: manual vendor download: {driver["source"]}', flush=True)
            if not args.all:
                failures.append(driver["id"])
        else:
            print(f'{driver["id"]}: downloading {driver["version"]}', flush=True)
            try:
                count = install(driver, destination, cache)
                print(f'{driver["id"]}: installed {count} JARs in {destination / driver["id"]}', flush=True)
            except (OSError, RuntimeError, subprocess.SubprocessError) as error:
                failures.append(driver["id"])
                print(f'{driver["id"]}: FAILED: {error}', flush=True)
    if failures:
        print("Uninstalled packages: " + ", ".join(failures), flush=True)
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
