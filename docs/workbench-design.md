# Rowcraft workbench design

Source-scoped navigation keeps query consoles, database objects, schema maps, and
query history close to the selected connection. Database trees use progressive
disclosure; opening one table should not expand every column in the source.
Refresh belongs beside the source and database controls.

Schema maps use introspected key constraints and disclose missing relationship
metadata. Result charts visualize query output with explicit axis choices and
identify when their input is limited to visible rows.

The desktop uses independent React components and a Tauri shell. Connection and
driver configuration are documented in [data source options](data-source-options.md).

The bundled JetBrains Mono fonts come directly from the official v2.304 release.
Source URLs and SHA-256 hashes are recorded in
`apps/rowcraft-desktop/public/fonts/README.md`; the SIL Open Font License is included
beside the fonts.
