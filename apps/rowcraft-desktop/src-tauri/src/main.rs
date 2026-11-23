// No console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // AI clients launch this same executable as `rowcraft-desktop mcp` (see the MCP panel).
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("mcp") {
        std::process::exit(rowcraft_desktop::run_mcp(args.iter().any(|a| a == "--allow-write")));
    }
    rowcraft_desktop::run();
}
