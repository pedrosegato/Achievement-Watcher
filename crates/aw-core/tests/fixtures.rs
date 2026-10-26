use std::fs;
use std::path::Path;

use aw_core::model::Unlock;
use aw_core::parsers::{Format, parse};

const FIXTURES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

fn format_for_dir(dir: &str) -> Format {
    match dir {
        "achiev_ini" => Format::AchievIni,
        "codex_ini" => Format::CodexIni,
        "creamapi" => Format::CreamApi,
        "goldberg_json" => Format::GoldbergJson,
        "onlinefix" => Format::OnlineFix,
        "reloaded_ini" => Format::ReloadedIni,
        "tenoke" => Format::TenokeUserStats,
        "generic_ini" => Format::GenericIni,
        "razor1911" => Format::Razor1911,
        "universelan" => Format::GenericIni,
        "generic_json" => Format::GenericJson,
        "rune_uplay" => Format::RuneCfg,
        other => panic!("unknown fixture dir {other}"),
    }
}

fn utf16le(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn utf16be(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_be_bytes).collect()
}

fn variants(lf: &str) -> Vec<(&'static str, Vec<u8>)> {
    let crlf = lf.replace('\n', "\r\n");
    let mut utf8_bom = vec![0xEF, 0xBB, 0xBF];
    utf8_bom.extend_from_slice(lf.as_bytes());
    let mut utf16le_bom = vec![0xFF, 0xFE];
    utf16le_bom.extend(utf16le(lf));
    let mut utf16be_bom = vec![0xFE, 0xFF];
    utf16be_bom.extend(utf16be(lf));
    vec![
        ("LF", lf.as_bytes().to_vec()),
        ("CRLF", crlf.into_bytes()),
        ("UTF-8 BOM", utf8_bom),
        ("UTF-16LE BOM", utf16le_bom),
        ("UTF-16LE no BOM", utf16le(lf)),
        ("UTF-16BE BOM", utf16be_bom),
    ]
}

#[test]
fn all_fixtures_match_expected_in_every_encoding() {
    let mut checked = 0;
    for dir in fs::read_dir(FIXTURES_DIR).unwrap() {
        let dir = dir.unwrap().path();
        if !dir.is_dir() {
            continue;
        }
        let format = format_for_dir(dir.file_name().unwrap().to_str().unwrap());
        for file in fs::read_dir(&dir).unwrap() {
            let path = file.unwrap().path();
            if path.to_str().unwrap().ends_with(".expected.json") {
                continue;
            }
            let expected = read_expected(&path);
            let lf = fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
            for (name, bytes) in variants(&lf) {
                let actual = parse(format, &bytes)
                    .unwrap_or_else(|e| panic!("{} [{name}]: {e}", path.display()));
                assert_eq!(actual, expected, "{} [{name}]", path.display());
            }
            checked += 1;
        }
    }
    assert!(checked > 0, "no fixtures found in {FIXTURES_DIR}");
}

fn read_expected(path: &Path) -> Vec<Unlock> {
    let expected_path = path.with_extension("expected.json");
    let json = fs::read_to_string(&expected_path)
        .unwrap_or_else(|e| panic!("{}: {e}", expected_path.display()));
    serde_json::from_str(&json).unwrap_or_else(|e| panic!("{}: {e}", expected_path.display()))
}
