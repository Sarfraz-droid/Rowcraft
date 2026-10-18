# DataGrip workbench reference

Inspected the user-selected `/Applications/DataGrip.app` on 2026-10-18 using REA and focused, read-only ZIP resource parsing. This is a navigation reference for the Rowcraft redesign; no proprietary implementation or visual assets were copied.

## Evidence

- REA `open_binary` resolved the bundle to its arm64 Mach-O launcher, `/Applications/DataGrip.app/Contents/MacOS/datagrip`, SHA-256 `51d96a4743c0c9d121eaf1102818434e1c88b5c582f93da7fc457541d4c1d681`.
- REA `inspect_artifact(detail: summary)` retained parent evidence `ev_8e3d39f967d7402eada5ed63e476b2ab28098cbd6d0a3e0a979cce4f09307f8a`; summary evidence `ev_08e46e76ca54bf0dad1550ce016fe33853464e451a2919592b554fa72a3729fc`; inventory evidence `ev_3b96929f0d5e9e10727dfc48fb07c152c326d8505fd919ccb2a383087c655c91`.
- Artifact manifest `agm_5a6a9deb1c524af395c468eb8a0c558bdc57f33a04dcaac0bf78a8e3e27ceb7c`: 2,317 nodes, 2,950 occurrences, 2,727 edges, no reported integrity contradictions. Root digest `3ffc12b15c0674831be6157dac5bce59604912a7beba0ab92a1caa6fc605070a`.
- Focused ZIP parsing of `Contents/plugins/DatabaseTools/lib/database-plugin.jar`, member `intellij.database.diagrams.xml`, observed database schema diagram and query-plan providers, plus actions registered in the Database View Diagrams group.
- Focused ZIP parsing of `Contents/plugins/DatabaseTools/lib/modules/intellij.database.jar`, member `messages/DatabaseBundle.properties`, observed labels for new query consoles, database schema diagrams, query history, refreshing selected objects, and refreshing the databases list.

## Design implications

Source-scoped navigation should keep query consoles, database objects, schema maps, and query history close to the selected connection. Database trees benefit from progressive disclosure; opening one table should not expand every column in the source. Refresh belongs beside the source and database controls. The source overview dashboard is a Rowcraft design addition, informed by the user's request, rather than a claimed DataGrip screen.

Schema maps must use introspected key constraints and clearly disclose missing relationship metadata. Results charts should visualize query output with explicit axis choices, and should describe visible/result-limited rows accurately.

## Font provenance

The unmodified, openly licensed JetBrains Mono Regular and Medium fonts were obtained from the installed DataGrip JBR fonts directory and placed in `apps/dg-desktop/public/fonts/`. The SIL Open Font License 1.1 is included beside them as `JetBrainsMono-OFL.txt`, obtained from the official `JetBrains/JetBrainsMono` repository. This font license is separate from the proprietary DataGrip application license.

## Limits

REA artifact inventory and resource labels are static observations. They establish the presence of packaged capabilities and labels, not executed behavior or exact live screen layout. Nested JAR resource parsing was restricted to the named XML and message resources. No full decompilation, runtime DataGrip UI capture, or claim of feature parity is made. The REA native session was closed after inspection.
