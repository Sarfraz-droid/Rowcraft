# Using the database workbench

The desktop workbench groups each source's tools in the sidebar. Select a source to connect and open its overview. Expand its row to open a specific view; the plus button in the document strip opens consoles, source views, and schema browsers. Documents from different sources can stay open together. Each has a close button. Tab source subtitles appear when documents span multiple sources; tooltips always identify the source.

| View | What it shows | Useful actions |
| --- | --- | --- |
| Overview | Inspected schema, object, column, and foreign-key counts; schema and column-type charts; recent queries | Refresh schema, browse objects, open a table preview |
| SQL console | Independent SQL editor; result table or chart appears after execution | Run SQL, resize the editor/results split, export displayed results as CSV |
| Schema map | Tables and views, types, key markers, and declared foreign-key relationships | Search tables, filter by schema, zoom, click a table heading to open its data document |
| Databases | Searchable tables and views with a column, constraint, and index inspector | Filter by schema, select an object, open its data document |
| Query history | Successful and failed statements for the selected source in this session | Search SQL or errors, load a statement into the console, clear this source's history |

Use the sidebar's add menu to configure a server, import an existing SQLite file, or manage custom drivers. SQLite imports and real connections require the desktop app. The browser development preview uses a mock backend. Connection and driver details are described in [the README](../README.md#open-an-existing-sqlite-file) and [data source options](data-source-options.md).

Press Cmd+K on macOS or Ctrl+K elsewhere to reveal the sidebar and focus source search. Cmd/Ctrl+T creates another console for the selected source; Cmd/Ctrl+W closes the active document. The tab strip also supports arrow keys, Home, End, and Delete when focused. In the SQL console, Cmd+Enter or Ctrl+Enter runs the selected SQL, or the full editor contents when nothing is selected. Loading a statement from history opens a new console; press Run to execute it.

Each console has a Database dropdown listing saved source connections. Changing it retains your SQL, clears that console's previous results and error, and connects to the selected source. It does not execute the SQL. Connection failures appear in the workspace; Run reconnects to the console's selected source before execution.

The active source's Tables group starts collapsed. Open it to search metadata and expand schemas and tables. A schema's separate open button creates a schema browser document. Opening a table creates or focuses its data document. Its Data mode uses the full pane for rows or charts; Structure shows reported columns, keys, defaults, constraints, and indexes. Open SQL creates a console with the preview query. Schema refresh reads the current source's catalog again. Counts and charts describe that inspection; they do not monitor database activity. The schema map connects visible objects using declared foreign keys and shows up to seven columns per card. Filters can hide relationship endpoints.

The result chart requires a numeric column. Choose label and value columns to plot returned rows without aggregation. It shows at most the first 50 finite numeric rows and identifies omitted rows. The CSV action exports the currently displayed query output, within the query's row limit; it does not stream an entire table.

Open documents, SQL drafts, query results, and the newest 200 history entries across sources live in memory for the current app session. Switching documents retains their editor, result view, splitter position, and filters while they remain open. Closing a document discards its workspace state. Clearing history affects only the selected source. Theme preference is saved locally when storage is available.

Use the workspace row theme control to switch between light and dark. The breadcrumb menu button hides or shows the sidebar to make more room for SQL or data. Narrow windows keep independently scrolling panes and simplify secondary hints.

The visual system is recorded in [DESIGN.md](../DESIGN.md), with component previews in [.impeccable/design.json](../.impeccable/design.json). These documents describe the implemented source; API-stub DOM interaction checks cover behavior, while browser screenshot verification remains pending because the browser daemon could not start in the sandbox.
