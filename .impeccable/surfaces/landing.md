# Rowcraft landing

Scope: standalone static HTML/CSS/JS in `apps/landing`. Mode: Persuade. Current user-confirmed direction matches Rowcraft’s dark desktop UI: charcoal canvas, flat panels, visible rules, Space Mono typography, square cobalt controls, existing white mark, and a Space Grotesk wordmark. “Built with Rust” remains explicit. Local design authority: `apps/landing/DESIGN.md`; leave the desktop’s root design documents unchanged.

Primary path: download or build from source. Product proof is the unchanged desktop screenshot (`assets/ui.jpg`), linked at full size. It is not an interactive browser workbench. No sculpture, perspective, floating backplates, or pointer tilt. Preserve OS selection, installation methods, command copying, keyboard access, native disclosures, and reduced-motion behavior. Download and repository destinations come from `config.js`; unpublished assets and registry commands remain honestly labeled.

The resource-use section uses the Rowcraft-only measurements bundled as `assets/benchmarks/connected-idle.json`: average observed process-tree CPU 0.18% and memory 35.5 MiB for Rowcraft 0.0.1. Show average and peak bars on fixed axes, numeric captions, the italic “Tested on Mac” label, and visible methodology. One shared-machine capture on macOS 26.6.2 ARM64 with 12 logical CPUs, 120 seconds after 30 seconds warmup, 98 samples; external WebKit helpers are excluded. These numbers do not measure query speed, startup speed, or total app footprint.

Verification for this redesign: desktop (1440px) and mobile (390px) screenshots reviewed; narrow 320px layout checked without overflow. OS/source controls passed browser checks. JavaScript syntax checks and five existing tests passed. No dependency installation was required.
