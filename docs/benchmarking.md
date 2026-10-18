# Compare Rowcraft and DataGrip on macOS

## Prepared comparison with matching database connections

The isolated setup is already prepared under `.dev/benchmark/` on this machine.
It contains a deterministic SQLite database (10,000 customers and 100,000 orders),
one read-only connection named **Benchmark SQLite** in each app, and matching
queries in `.dev/benchmark/datagrip-project/workload.sql`.

Launch these profiles from Terminal:

```sh
python3 scripts/setup-benchmark.py launch
```

DataGrip may show first-run/license prompts. Do not import your regular settings
when prompted. In each app, connect **Benchmark SQLite**, wait for introspection
to finish, and leave it idle. The configured source is not automatically a live
connection. If DataGrip asks about the project, trust this generated local project
to enable its database features. Then run:

```sh
python3 scripts/setup-benchmark.py measure --label connected-idle --warmup 30 --duration 120
```

This targets the launched PIDs, excluding your normal instances even if they are
running with other databases. For less background interference, close your regular
instances yourself after saving any work. The launcher does not close anything.
The benchmark profiles remain open afterward; close them normally when finished.
For an unconnected baseline, run `measure --label disconnected-idle` before
connecting either source. For query comparisons, paste the same statement from
`workload.sql` into each app's SQL console, show at most 1000 rows, and perform
the same actions during a separately labelled capture.

The DataGrip profile uses separate config, system, plugins and log directories
through `DATAGRIP_PROPERTIES`, as described in the
[JetBrains configuration documentation](https://www.jetbrains.com/help/datagrip/tuning-the-ide.html)
and [IDE directory settings](https://www.jetbrains.com/help/datagrip/directories-used-by-the-ide-to-store-settings-caches-plugins-and-logs.html).
Only the existing SQLite JDBC JARs are copied; user connections and credentials
are not copied. Rowcraft uses `DGLITE_CONFIG_DIR`. This is a clean-profile
comparison and will not include your normal DataGrip's user-installed plugins.
WebKit helper attribution still has the limitation described below; helper PID
flags can also be passed to `setup-benchmark.py measure`.

To prepare the same fixture on another machine with a SQLite JDBC driver already
installed in DataGrip, run `python3 scripts/setup-benchmark.py prepare` first.
For a second independent fixture, pass `--directory /absolute/path/to/new-fixture`
to all three commands. Preparation refuses to overwrite a launched session.

## Measure regular app instances

Run from Terminal with Python 3.9 or newer. No packages need to be installed.
The default bundles are `target/release/bundle/macos/Rowcraft.app` and
`/Applications/DataGrip.app`. Build Rowcraft with `make installer` if needed.

```sh
python3 scripts/benchmark-apps.py --launch
```

This opens missing apps, waits 15 seconds, then samples both apps together for
60 seconds. It leaves both apps open. Without `--launch`, both must already be
running. The script never quits apps or changes their settings.

For a prepared workload, open both apps, connect to the same database, use the
same schemas, query, row limit, and window size, and let indexing finish. Then run:

```sh
python3 scripts/benchmark-apps.py \
  --label 'connected-idle' --warmup 30 --duration 120 --interval 1
```

For active usage, repeat the same operations in each app during the capture.
This script measures resource use; it does not automate queries or UI actions.
For reproducible idle comparisons, keep both apps visible, avoid other work,
use the same power mode, and repeat at least three times. Simultaneous active
workloads compete for machine resources; treat these as shared-machine
observations rather than isolated performance or query-latency measurements.
The settling period excludes startup, so these reports do not measure launch cost.

Override bundle locations or the report destination:

```sh
python3 scripts/benchmark-apps.py \
  --rowcraft '/Applications/Rowcraft.app' \
  --datagrip '/Applications/DataGrip.app' \
  --output benchmark-results/connected-idle-run-1
```

The output directory must be new. Defaults go into an ignored timestamped
directory under `benchmark-results/`. Each capture prints a comparison and saves:

- `samples.csv`: elapsed time, actual interval length, CPU percent, CPU seconds,
  RSS memory in MiB, process count, and included PIDs for each app.
- `summary.json`: average and peak CPU and RSS, median RSS, app versions, machine
  information, workload label, capture status, and observed process paths.

Ctrl+C during measurement writes the samples collected so far and marks the
report incomplete. An app exit also marks the report incomplete and returns a
nonzero exit status. Compare only complete captures of the same workload.

## What the numbers include

CPU is calculated from differences in cumulative process CPU time between
snapshots, divided by the actual elapsed interval. **100% means one logical CPU
fully occupied**; multithreaded work can exceed 100%. Average CPU is weighted
by interval length. Peak CPU is the highest interval average, not an instantaneous
peak. BSD `ps` CPU-time precision can make very short intervals noisy.

Memory is the sum of sampled resident set sizes (RSS), in MiB, for the app's main
process and observed descendants. Average RSS is weighted by interval length.
It is not JVM heap size, macOS physical footprint, or unique memory: shared pages
may be counted more than once, and swapped/compressed allocations are not fully
represented. Peaks between samples and processes that start and exit between
samples can be missed. A process's CPU use before its first observation is excluded.

Descendants include JVM/driver/helper processes parented beneath either app.
Previously observed descendants remain tracked if reparented. However, **macOS
can launch WebKit helpers through system services outside Rowcraft's process
tree**. These are not automatically attributed, so the default RSS can undercount
Rowcraft's total memory. Do not publish it as a complete app-footprint comparison
without checking helper coverage. In Activity Monitor, use the hierarchical
process view and inspect helper ownership; add only PIDs verified to belong to
the corresponding app:

```sh
python3 scripts/benchmark-apps.py \
  --rowcraft-helper-pid 12345 --rowcraft-helper-pid 12346
```

`--datagrip-helper-pid` works the same way. Explicit helpers and their descendants
are included; the script rejects overlapping attribution. Check the observed
process inventory in `summary.json` after each run. Helpers restarted outside
the tree need their new PIDs supplied in a new capture. External database servers
and unrelated system services are excluded.

Validation for the sampler and report writer:

```sh
python3 -m unittest discover -s scripts -p 'test_benchmark_apps.py'
```
