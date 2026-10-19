import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import xml.etree.ElementTree as ET
import zipfile

spec = importlib.util.spec_from_file_location("download_drivers", Path(__file__).with_name("download-drivers.py"))
downloader = importlib.util.module_from_spec(spec)
spec.loader.exec_module(downloader)


class DriverDownloadsTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.driver = dict(id="sample", group="org.example", artifact="sample", version="1.0", source="https://repo.maven.apache.org/maven2")
        self.existing = self.root / "sample"
        self.existing.mkdir()
        (self.existing / "old.jar").write_bytes(b"old release")

    def test_failed_download_preserves_installed_release(self):
        with patch.object(downloader.subprocess, "run", side_effect=subprocess.CalledProcessError(1, "mvn")):
            with self.assertRaises(subprocess.CalledProcessError):
                downloader.install(self.driver, self.root, self.root / "cache")
        self.assertEqual((self.existing / "old.jar").read_bytes(), b"old release")
        self.assertFalse(list(self.root.glob(".download-*")))

    def test_empty_resolution_preserves_installed_release(self):
        with patch.object(downloader.subprocess, "run"):
            with self.assertRaisesRegex(RuntimeError, "no driver files"):
                downloader.install(self.driver, self.root, self.root / "cache")
        self.assertTrue((self.existing / "old.jar").is_file())

    def test_success_replaces_stale_jars_and_records_checksums(self):
        def resolve(command, **kwargs):
            destination = Path(next(arg.split("=", 1)[1] for arg in command if arg.startswith("-DoutputDirectory=")))
            destination.mkdir()
            with zipfile.ZipFile(destination / "new.jar", "w") as archive:
                archive.writestr("META-INF/services/java.sql.Driver", "org.example.Driver\n")
                archive.writestr("org/example/Driver.class", b"driver class")
            with zipfile.ZipFile(destination / "dependency.jar", "w") as archive:
                archive.writestr("dependency.class", b"dependency")
        with patch.object(downloader.subprocess, "run", side_effect=resolve):
            self.assertEqual(downloader.install(self.driver, self.root, self.root / "cache"), 2)
        self.assertFalse((self.existing / "old.jar").exists())
        receipt = json.loads((self.existing / "download-receipt.json").read_text())
        self.assertEqual(receipt["files"]["new.jar"], hashlib.sha256((self.existing / "new.jar").read_bytes()).hexdigest())

    def test_placeholder_package_preserves_installed_release(self):
        def resolve(command, **kwargs):
            destination = Path(next(arg.split("=", 1)[1] for arg in command if arg.startswith("-DoutputDirectory=")))
            destination.mkdir()
            with zipfile.ZipFile(destination / "placeholder.jar", "w") as archive:
                archive.writestr("README", "placeholder")
        with patch.object(downloader.subprocess, "run", side_effect=resolve):
            with self.assertRaisesRegex(RuntimeError, "placeholder"):
                downloader.install(self.driver, self.root, self.root / "cache")
        self.assertTrue((self.existing / "old.jar").is_file())

    def test_catalog_covers_all_builtin_ids_and_pins_published_versions(self):
        root = downloader.ROOT
        catalog = json.loads(downloader.CATALOG.read_text())["drivers"]
        ui = json.loads((root / "apps/rowcraft-desktop/src/driver-catalog.json").read_text())
        self.assertEqual({driver["id"] for driver in catalog}, {driver["id"] for driver in ui})
        for driver in catalog:
            if driver["distribution"] == "maven":
                metadata = ET.parse(root / "drivers/upstream" / (driver["id"] + ".xml"))
                self.assertIn(driver["version"], [v.text for v in metadata.findall("versioning/versions/version")])
            elif driver["distribution"] == "vendor":
                self.assertTrue(driver["source"].startswith("https://"))


if __name__ == "__main__":
    unittest.main()
