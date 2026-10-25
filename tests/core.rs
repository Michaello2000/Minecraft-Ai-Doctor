use minecraft_ai_doctor::{analyze, logs, privacy};
use std::path::Path;

#[test]
fn fixture_diagnoses_and_exact_lines() {
    for (file, expected) in [
        ("missing", "dependency"),
        ("duplicate", "duplicate"),
        ("version", "version"),
        ("java", "java"),
        ("mixin", "mixin"),
    ] {
        let path = format!("tests/fixtures/{file}.log");
        let report = analyze(Path::new(&path)).unwrap();
        assert!(
            report.findings.iter().any(|f| f.code == expected),
            "{file}: {:?}",
            report.findings
        );
        for finding in &report.findings {
            for evidence in &finding.evidence {
                let source = report
                    .logs
                    .iter()
                    .find(|l| l.source == evidence.source)
                    .unwrap();
                assert!(
                    source
                        .lines
                        .iter()
                        .any(|l| l.number == evidence.line && l.text == evidence.text)
                );
            }
        }
    }
    let report = analyze(Path::new("tests/fixtures/success.log")).unwrap();
    assert!(!report.likely_crash());
    assert!(report.findings.iter().any(|f| f.code == "normal_exit"));
}

#[test]
fn privacy_and_malformed_bytes() {
    let text = r"C:\Users\Alice\game accessToken=SECRET 192.168.1.2 https://example.com/?key=secret Player: Alice";
    let safe = privacy::redact(text);
    for secret in ["Alice", "SECRET", "192.168.1.2", "example.com"] {
        assert!(!safe.contains(secret), "{safe}");
    }
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("latest.log");
    std::fs::write(&path, [0xff, b'\n', b'x']).unwrap();
    let log = logs::read(&path).unwrap();
    assert_eq!(log.lines[1].number, 2);
}

#[test]
fn cli_offline_json() {
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_minecraft-ai-doctor"))
        .args([
            "analyze",
            "--log",
            "tests/fixtures/missing.log",
            "--offline",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(json["findings"][0]["code"], "dependency");
}

#[test]
fn pasted_console_keeps_each_specific_dependency_issue() {
    let text = include_str!("fixtures/detailed.log");
    let report = minecraft_ai_doctor::analyze_text(text).unwrap();
    assert_eq!(
        report
            .findings
            .iter()
            .filter(|f| f.code == "dependency")
            .count(),
        2
    );
    assert_eq!(
        report
            .findings
            .iter()
            .filter(|f| f.code == "version")
            .count(),
        1
    );
    assert_eq!(report.logs[0].source, "Pasted console");
    assert_eq!(report.logs[0].lines[2].number, 3);
    assert!(minecraft_ai_doctor::analyze_text("  \n").is_err());
    let huge = format!("{}\n", "x".repeat(logs::MAX_BYTES + 20));
    assert!(minecraft_ai_doctor::analyze_text(&huge).unwrap().logs[0].truncated);
    let version_only = text.lines().take(3).collect::<Vec<_>>().join("\n");
    let report = minecraft_ai_doctor::analyze_text(&version_only).unwrap();
    assert_eq!(
        report
            .findings
            .iter()
            .filter(|f| f.code == "version")
            .count(),
        1
    );
    assert!(!report.findings.iter().any(|f| f.code == "dependency"));
}

#[test]
fn file_limits_export_safety_and_cancel() {
    use std::{
        io::{Seek, SeekFrom, Write},
        sync::atomic::AtomicBool,
    };
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("latest.log");
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(b"first line\n").unwrap();
    file.seek(SeekFrom::Start(logs::MAX_BYTES as u64 + 3))
        .unwrap();
    file.write_all(b"x").unwrap();
    drop(file);
    let log = logs::read(&path).unwrap();
    assert!(log.truncated);
    assert_eq!(log.lines[0].number, 1);
    assert!(log.lines.len() <= logs::MAX_LINES);
    assert!(minecraft_ai_doctor::analyze_cancellable(&path, &AtomicBool::new(true)).is_err());
    let output = temp.path().join("report.md");
    minecraft_ai_doctor::ui::export_new(&output, "redacted report").unwrap();
    assert!(minecraft_ai_doctor::ui::export_new(&output, "replacement").is_err());
    assert_eq!(std::fs::read_to_string(output).unwrap(), "redacted report");
    assert!(
        minecraft_ai_doctor::ui::export_protected(
            &temp.path().join("blocked.md"),
            "data",
            &[temp.path().to_path_buf()]
        )
        .is_err()
    );
    let launcher = temp.path().join("PrismLauncher");
    std::fs::create_dir(&launcher).unwrap();
    assert!(minecraft_ai_doctor::ui::export_new(&launcher.join("report.md"), "data").is_err());
}

#[test]
fn discovery_versions_and_cli_errors() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    std::fs::create_dir(root.join("minecraft")).unwrap();
    std::fs::create_dir(root.join("minecraft/mods")).unwrap();
    std::fs::write(root.join("mmc-pack.json"),r#"{"components":[{"uid":"net.minecraft","version":"1.21.1"},{"uid":"net.fabricmc.fabric-loader","version":"0.16.9"}]}"#).unwrap();
    for path in [root.to_path_buf(), root.join("minecraft")] {
        let inputs = minecraft_ai_doctor::discovery::inputs(&path).unwrap();
        assert_eq!(inputs.versions["minecraft"], "1.21.1");
        assert_eq!(inputs.versions["fabricloader"], "0.16.9");
        assert!(inputs.mods.is_some());
    }
    for args in [
        vec!["analyze"],
        vec!["unknown"],
        vec!["analyze", "--log", "missing.log"],
    ] {
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_minecraft-ai-doctor"))
            .args(args)
            .output()
            .unwrap();
        assert!(!result.status.success());
    }
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_minecraft-ai-doctor"))
        .args(["--from-prism"])
        .env_remove("INST_MC_DIR")
        .env_remove("INST_DIR")
        .output()
        .unwrap();
    assert!(!status.status.success());
    assert!(String::from_utf8_lossy(&status.stderr).contains("missing"));
}

#[test]
fn hook_does_not_open_on_a_successful_exit() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("logs")).unwrap();
    std::fs::copy(
        "tests/fixtures/success.log",
        temp.path().join("logs/latest.log"),
    )
    .unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_minecraft-ai-doctor"))
        .args(["--from-prism", "--only-crashes"])
        .env("INST_MC_DIR", temp.path())
        .env_remove("INST_DIR")
        .output()
        .unwrap();
    assert!(result.status.success());
    assert!(result.stdout.is_empty());
}

#[cfg(windows)]
#[test]
fn reject_junction_escapes() {
    use std::os::windows::fs::symlink_dir;
    let temp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let link = temp.path().join("linked");
    // Windows developer mode or privilege is needed to create a symlink; if unavailable,
    // ordinary-path checks still run and the exact OS limitation is printed.
    match symlink_dir(outside.path(), &link) {
        Ok(()) => assert!(minecraft_ai_doctor::discovery::safe_path(&link).is_err()),
        Err(e) if e.raw_os_error() == Some(1314) => {
            eprintln!("Symlink fixture unavailable: {e}");
            assert!(minecraft_ai_doctor::discovery::safe_path(temp.path()).is_ok());
        }
        Err(e) => panic!("{e}"),
    }
}
