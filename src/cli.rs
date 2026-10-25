use crate::{Result, discovery, privacy, ui};
use std::{ffi::OsString, path::PathBuf};

const HELP: &str = "Minecraft Ai Doctor 0.1.0\n\nNo arguments: open the desktop app.\nanalyze --log <file.log|file.txt> --offline [--json]\nanalyze --instance <folder> --offline [--json]\nanalyze --mods <mods-folder> --offline [--json]\ndiscover [--root <custom-instances-folder>]\n--from-prism [--only-crashes] [--max-log-age-seconds <seconds>]\nlab-plan --instance <folder>\n--smoke-gui [--log <file>] (renders three frames and exits)\n\nAll analysis is read-only. Cloud sending is available only through the GUI privacy preview.";

fn take_path(args: &[OsString], flag: &str) -> Option<PathBuf> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
}

pub fn run() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{HELP}");
        return Ok(());
    }
    if args.iter().any(|a| a == "--version") {
        println!("Minecraft Ai Doctor {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.first().is_some_and(|a| a == "discover") {
        let root = take_path(&args, "--root");
        let json = serde_json::to_string_pretty(&discovery::discover(root.as_deref()))
            .map_err(|e| e.to_string())?;
        println!("{}", privacy::redact(&json));
        return Ok(());
    }
    if args.first().is_some_and(|a| a == "lab-plan") {
        let path = take_path(&args, "--instance").ok_or("lab-plan requires --instance <folder>")?;
        let plan = crate::lab::plan(&path)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&plan).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    let hook = args.iter().any(|a| a == "--from-prism");
    let smoke = args.iter().any(|a| a == "--smoke-gui");
    let path = take_path(&args, "--log")
        .or_else(|| take_path(&args, "--instance"))
        .or_else(|| take_path(&args, "--mods"));
    let path = if hook {
        Some(discovery::hook_path()?)
    } else {
        path
    };
    if args.first().is_some_and(|a| a == "analyze") {
        let path = path.ok_or("analyze requires --log, --instance or --mods <path>")?;
        let report = crate::analyze(&path)?;
        if args.iter().any(|a| a == "--json") {
            println!("{}", privacy::report_json(&report)?);
        } else {
            println!("{}", report.markdown());
        }
        return Ok(());
    }
    if hook && args.iter().any(|a| a == "--only-crashes") {
        let root = path.as_deref().ok_or("No Prism instance path")?;
        let inputs = discovery::inputs(root)?;
        let max_age = take_path(&args, "--max-log-age-seconds")
            .and_then(|v| v.to_str().and_then(|s| s.parse::<u64>().ok()))
            .unwrap_or(300);
        let latest = inputs
            .logs
            .iter()
            .find(|p| p.file_name().is_some_and(|n| n == "latest.log"));
        let Some(latest) = latest else {
            return Ok(());
        };
        let age = std::fs::metadata(latest)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok());
        if age.is_none_or(|age| age.as_secs() > max_age) {
            return Ok(());
        }
        let log = crate::logs::read(latest)?;
        if !crate::rules::diagnose(&log)
            .iter()
            .any(|f| f.severity == "error")
        {
            return Ok(());
        }
    }
    if !args.is_empty() && !hook && !smoke {
        return Err(format!("Unrecognized command.\n{HELP}"));
    }
    ui::run(path, smoke)?;
    if smoke {
        println!("GUI smoke completed: rendered three frames and closed normally.");
    }
    Ok(())
}
