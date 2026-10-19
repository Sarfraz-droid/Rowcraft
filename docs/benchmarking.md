# Rowcraft resource measurements

The landing page stores a Rowcraft-only snapshot in
`apps/landing/assets/benchmarks/connected-idle.json`. It records Rowcraft 0.0.1
with one SQLite database connected and the app idle on macOS 26.6.2 ARM64.
The capture used a 30-second warmup, roughly 120 seconds of observation, and
98 process-tree samples on a machine with 12 logical CPUs.

The page shows average and peak observed memory (RSS) and CPU. Averages are
weighted by sampling interval. Memory is summed resident set size in MiB for
the main process and observed descendants. CPU derives from elapsed process
CPU time; 100% means one logical CPU fully occupied. Peak CPU is the highest
sampled interval average, not an instantaneous peak.

These are historical measurements of the recorded version. They do not measure
query latency, startup speed, or complete app footprint. The capture ran on a
shared machine; background work can affect results. External WebKit helpers
were excluded, so memory is undercounted. RSS can count shared pages more than
once and does not fully represent compressed or swapped allocations.

For a new capture, build Rowcraft with `make installer`, open a dedicated
connection fixture, and let introspection finish. Keep the window size,
connection, query limits, power mode, and other workloads consistent. Sample
the app's main process and verified descendants; report helper coverage and
repeat runs. Keep raw samples, durations, machine details, app version, and
limitations with each published result. Do not present old measurements as
measurements of a new release.
