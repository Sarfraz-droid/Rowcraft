# Database driver catalog

`catalog.json` is Rowcraft's saved database and download manifest. All 58 built-in
database IDs are listed here. Native entries use the Rust connectors. Maven entries
pin a package version and its upstream metadata URL. Vendor entries require a
manual download from the linked supplier.

`upstream/*.xml` contains release lists downloaded directly from Maven Central.
The pinned package need not be the latest release: package compatibility matters
(for example, Ignite's 2.x client). These files contain upstream package metadata.

Install packages with `python3 scripts/download-drivers.py --all` from the repository
root. See [driver setup](../docs/jdbc-drivers.md) for storage and Java requirements.
The catalog records download availability, not proof that every vendor connection
has been tested. Native protocol variants keep their documented support boundaries.
