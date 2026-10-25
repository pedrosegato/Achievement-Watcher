use std::process::Command;

fn fake_root_with_elden_ring() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp
        .path()
        .join("PublicDocuments/OnlineFix/1245620/Stats/Achievements.ini");
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, "[ACH39]\nachieved=true\ntimestamp=1761321600\n").unwrap();
    tmp
}

fn aw_cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_aw-cli"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn scan_json_lists_fake_onlinefix_game() {
    let tmp = fake_root_with_elden_ring();
    let out = aw_cli(&[
        "scan",
        "--json",
        "--fake-root",
        tmp.path().to_str().unwrap(),
    ]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v[0]["app_id"], 1245620);
    assert_eq!(v[0]["unlocks"][0]["api_name"], "ACH39");
    assert_eq!(v[0]["unlocks"][0]["unlocked_at"], 1761321600);
    assert_eq!(v[0]["files"][0]["outcome"]["status"], "parsed");
}

#[test]
fn scan_lists_reloaded_game_from_programdata() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp
        .path()
        .join("ProgramData/Steam/Player/70/stats/achievements.ini");
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, "[A1]\nState=01000000\nTime=00f15365\n").unwrap();
    let out = aw_cli(&[
        "scan",
        "--json",
        "--fake-root",
        tmp.path().to_str().unwrap(),
    ]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v[0]["app_id"], 70);
    assert_eq!(v[0]["files"][0]["source_id"], "reloaded_programdata");
    assert_eq!(v[0]["unlocks"][0]["api_name"], "A1");
    assert_eq!(v[0]["unlocks"][0]["unlocked_at"], 1700000000);
}

#[test]
fn scan_human_output_shows_game_and_achievement() {
    let tmp = fake_root_with_elden_ring();
    let out = aw_cli(&["scan", "--fake-root", tmp.path().to_str().unwrap()]);
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("1245620  unlocked 1  files 1"), "{text}");
    assert!(text.contains("+ ACH39  "), "{text}");
}

#[test]
fn scan_with_nothing_found_says_so() {
    let tmp = tempfile::tempdir().unwrap();
    let out = aw_cli(&["scan", "--fake-root", tmp.path().to_str().unwrap()]);
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stdout).unwrap().trim(),
        "No achievement files found."
    );
}

#[test]
fn scan_help_describes_flags() {
    let out = aw_cli(&["scan", "--help"]);
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("Print the result as JSON"), "{text}");
    assert!(text.contains("fake system root"), "{text}");
}

#[test]
fn scan_with_missing_fake_root_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("does-not-exist");
    let out = aw_cli(&["scan", "--fake-root", missing.to_str().unwrap()]);
    assert!(!out.status.success());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains(&format!(
            "aw-cli: fake root does not exist: {}",
            missing.display()
        )),
        "{stderr}"
    );
}

#[test]
fn scan_reports_file_error_and_still_exits_zero() {
    let tmp = tempfile::tempdir().unwrap();
    let empty = tmp
        .path()
        .join("PublicDocuments/OnlineFix/7/Achievements.ini");
    let good = tmp
        .path()
        .join("RoamingAppData/GSE Saves/7/achievements.json");
    for path in [&empty, &good] {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    }
    std::fs::write(&empty, "").unwrap();
    std::fs::write(&good, r#"{"A":{"earned":true,"earned_time":1761321600}}"#).unwrap();
    let out = aw_cli(&["scan", "--fake-root", tmp.path().to_str().unwrap()]);
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("onlinefix  error: empty file"), "{text}");
    assert!(text.contains("gse  ok (1)"), "{text}");
    assert!(text.contains("+ A  "), "{text}");
}

fn touch(root: &std::path::Path, rel: &str, body: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, body).unwrap();
}

fn vdf_path(p: &std::path::Path) -> String {
    p.display().to_string().replace('\\', "\\\\")
}

#[test]
fn scan_finds_tenoke_game_under_games_root() {
    let tmp = tempfile::tempdir().unwrap();
    let roots = tmp.path().join("Jogos");
    touch(&roots, "Elden/tenoke.ini", "[TENOKE]\nid = 1245620\n");
    touch(
        &roots,
        "Elden/SteamData/user_stats.ini",
        "[ACHIEVEMENTS]\n\"ACH39\" = {unlocked = true, time = 1761321600}\n",
    );
    let out = aw_cli(&[
        "scan",
        "--json",
        "--no-steam",
        "--fake-root",
        tmp.path().to_str().unwrap(),
        "--games-root",
        roots.to_str().unwrap(),
    ]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v[0]["app_id"], 1245620);
    assert_eq!(v[0]["files"][0]["source_id"], "tenoke");
    assert_eq!(v[0]["unlocks"][0]["api_name"], "ACH39");
}

#[test]
fn scan_uses_steam_manifest_with_fake_root() {
    let tmp = tempfile::tempdir().unwrap();
    let steam = tmp.path().join("Steam");
    touch(&steam, "steam.exe", "");
    touch(
        &steam,
        "steamapps/libraryfolders.vdf",
        &format!(
            "\"libraryfolders\" {{ \"0\" {{ \"path\" \"{}\" }} }}",
            vdf_path(&steam)
        ),
    );
    touch(
        &steam,
        "steamapps/appmanifest_480.acf",
        "\"AppState\" { \"appid\" \"480\" \"name\" \"Spacewar\" \"StateFlags\" \"4\" \"installdir\" \"Spacewar\" }",
    );
    touch(
        &steam,
        "steamapps/common/Spacewar/SteamData/user_stats.ini",
        "[ACHIEVEMENTS]\n\"A\" = {unlocked = true, time = 1761321600}\n",
    );
    let out = aw_cli(&[
        "scan",
        "--json",
        "--fake-root",
        tmp.path().to_str().unwrap(),
    ]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v[0]["app_id"], 480);
    let none = aw_cli(&[
        "scan",
        "--no-steam",
        "--fake-root",
        tmp.path().to_str().unwrap(),
    ]);
    assert_eq!(
        String::from_utf8(none.stdout).unwrap().trim(),
        "No achievement files found."
    );
}

#[test]
fn scan_reports_unresolved_in_human_output() {
    let tmp = tempfile::tempdir().unwrap();
    let roots = tmp.path().join("Jogos");
    touch(
        &roots,
        "Mystery/SteamData/user_stats.ini",
        "[ACHIEVEMENTS]\n",
    );
    let out = aw_cli(&[
        "scan",
        "--no-steam",
        "--fake-root",
        tmp.path().to_str().unwrap(),
        "--games-root",
        roots.to_str().unwrap(),
    ]);
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("Unresolved:"), "{text}");
    assert!(text.contains("? tenoke  "), "{text}");
    assert!(text.contains("(app id not found)"), "{text}");
}

#[test]
fn scan_rejects_missing_games_root() {
    let tmp = tempfile::tempdir().unwrap();
    let out = aw_cli(&[
        "scan",
        "--no-steam",
        "--fake-root",
        tmp.path().to_str().unwrap(),
        "--games-root",
        tmp.path().join("nope").to_str().unwrap(),
    ]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8(out.stderr)
            .unwrap()
            .contains("games root does not exist")
    );
}

#[test]
fn scan_rejects_drive_root_as_games_root() {
    let tmp = tempfile::tempdir().unwrap();
    let drive_root = tmp.path().ancestors().last().unwrap();
    let out = aw_cli(&[
        "scan",
        "--no-steam",
        "--fake-root",
        tmp.path().to_str().unwrap(),
        "--games-root",
        drive_root.to_str().unwrap(),
    ]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8(out.stderr)
            .unwrap()
            .contains("must not be a drive root")
    );
}

#[test]
fn watch_prints_first_boot_summary_then_popup() {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    let tmp = fake_root_with_elden_ring();
    let db = tmp.path().join("watch.sqlite");
    let mut child = Command::new(env!("CARGO_BIN_EXE_aw-cli"))
        .args([
            "watch",
            "--no-steam",
            "--db",
            db.to_str().unwrap(),
            "--fake-root",
            tmp.path().to_str().unwrap(),
        ])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if tx.send(line.unwrap()).is_err() {
                break;
            }
        }
    });
    let first = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(first, "First boot: imported 1 achievements in 1 games");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    std::fs::write(
        tmp.path()
            .join("PublicDocuments/OnlineFix/1245620/Stats/Achievements.ini"),
        format!("[ACH39]\nachieved=true\ntimestamp=1761321600\n[ACH40]\nachieved=true\ntimestamp={now}\n"),
    )
    .unwrap();
    let line = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    child.kill().unwrap();
    let _ = child.wait();
    assert!(line.starts_with("+ 1245620 ACH40 "), "{line}");
    assert!(line.ends_with(" popup"), "{line}");
}
