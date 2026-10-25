use std::collections::BTreeSet;
use std::error::Error;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::mpsc::channel;

use aw_core::discovery::detect::{DetectConfig, detect};
use aw_core::discovery::folders::KnownFolders;
use aw_core::discovery::gamediscovery::Unresolved;
use aw_core::discovery::steam::{pick_steam_path, steam_install_path};
use aw_core::engine::{CoreSettings, Engine, SyncOutcome};
use aw_core::model::Unlock;
use aw_core::scan::{FileOutcome, FileResult, GameScan, scan};
use aw_core::store::Store;
use aw_core::watcher::{self, WatchEvent, WatchOptions};
use chrono::{DateTime, Local, Utc};
use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "aw-cli",
    version,
    about = "Achievement Watcher detection tools"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "List achievement files and unlocks found on this machine")]
    Scan {
        #[arg(long, help = "Print the result as JSON")]
        json: bool,
        #[command(flatten)]
        sources: SourceArgs,
    },
    #[command(about = "Watch achievement files and print new unlocks as they happen")]
    Watch {
        #[arg(
            long,
            value_name = "FILE",
            help = "SQLite database file (default: aw-cli.sqlite in the current folder)"
        )]
        db: Option<PathBuf>,
        #[command(flatten)]
        sources: SourceArgs,
    },
}

#[derive(Args)]
struct SourceArgs {
    #[arg(
        long,
        value_name = "DIR",
        help = "Use DIR as a fake system root instead of the real Windows folders"
    )]
    fake_root: Option<PathBuf>,
    #[arg(
        long = "games-root",
        value_name = "DIR",
        help = "Folder whose subfolders are games (repeatable)"
    )]
    games_roots: Vec<PathBuf>,
    #[arg(
        long = "game-dir",
        value_name = "DIR",
        help = "A single game folder (repeatable)"
    )]
    game_dirs: Vec<PathBuf>,
    #[arg(long, help = "Do not read Steam libraries")]
    no_steam: bool,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("aw-cli: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn Error>> {
    match cli.command {
        Command::Scan { json, sources } => run_scan(json, sources),
        Command::Watch { db, sources } => run_watch(db, sources),
    }
}

fn resolve_sources(sources: SourceArgs) -> Result<(KnownFolders, DetectConfig), Box<dyn Error>> {
    validate_game_folders(&sources.games_roots, &sources.game_dirs)?;
    let folders = match &sources.fake_root {
        Some(root) if !root.exists() => {
            return Err(format!("fake root does not exist: {}", root.display()).into());
        }
        Some(root) => KnownFolders::under(root),
        None => KnownFolders::from_system()?,
    };
    let steam = match (&sources.fake_root, sources.no_steam) {
        (_, true) => None,
        (Some(root), false) => pick_steam_path([root.join("Steam")]),
        (None, false) => steam_install_path(),
    };
    Ok((
        folders,
        DetectConfig {
            steam_libraries: steam.is_some(),
            steam_stats: steam.is_some(),
            steam,
            games_roots: sources.games_roots,
            game_dirs: sources.game_dirs,
        },
    ))
}

fn run_scan(json: bool, sources: SourceArgs) -> Result<(), Box<dyn Error>> {
    let (folders, config) = resolve_sources(sources)?;
    let detection = detect(&folders, &config);
    let games = scan(&detection.found, Utc::now().timestamp());
    if json {
        println!("{}", serde_json::to_string_pretty(&games)?);
    } else {
        print_games(&games, &detection.unresolved);
    }
    Ok(())
}

fn run_watch(db: Option<PathBuf>, sources: SourceArgs) -> Result<(), Box<dyn Error>> {
    let (folders, detect) = resolve_sources(sources)?;
    let store = Store::open(&db.unwrap_or_else(|| PathBuf::from("aw-cli.sqlite")))?;
    let engine = Engine::new(
        store,
        folders,
        CoreSettings {
            detect,
            ..CoreSettings::default()
        },
    );
    let (events, inbox) = channel();
    let _watcher = watcher::start(engine, events, WatchOptions::default());
    for event in inbox {
        match event {
            WatchEvent::Synced(outcome) => print_outcome(&outcome),
            WatchEvent::ScanProgress(_) => {}
            WatchEvent::Error(error) => eprintln!("aw-cli: {error}"),
        }
    }
    Ok(())
}

fn print_outcome(outcome: &SyncOutcome) {
    if outcome.first_boot {
        let games: BTreeSet<u32> = outcome.new_unlocks.iter().map(|u| u.app_id).collect();
        println!(
            "First boot: imported {} achievements in {} games",
            outcome.new_unlocks.len(),
            games.len()
        );
        return;
    }
    for unlock in &outcome.new_unlocks {
        let time = local_time(unlock.unlocked_at).unwrap_or_else(|| "(no time)".to_owned());
        let kind = if unlock.notify { "popup" } else { "silent" };
        println!("+ {} {} {time} {kind}", unlock.app_id, unlock.api_name);
    }
}

fn validate_game_folders(games_roots: &[PathBuf], game_dirs: &[PathBuf]) -> Result<(), String> {
    for root in games_roots {
        if !root.is_dir() {
            return Err(format!("games root does not exist: {}", root.display()));
        }
        if root.parent().is_none() {
            return Err(format!(
                "games root must not be a drive root: {}",
                root.display()
            ));
        }
    }
    for dir in game_dirs {
        if !dir.is_dir() {
            return Err(format!("game dir does not exist: {}", dir.display()));
        }
    }
    Ok(())
}

fn print_games(games: &[GameScan], unresolved: &[Unresolved]) {
    if games.is_empty() && unresolved.is_empty() {
        println!("No achievement files found.");
        return;
    }
    for game in games {
        let unlocked = game.unlocks.iter().filter(|unlock| unlock.unlocked).count();
        println!(
            "{}  unlocked {unlocked}  files {}",
            game.app_id,
            game.files.len()
        );
        for file in &game.files {
            print_file(file);
        }
        for unlock in &game.unlocks {
            print_unlock(unlock);
        }
    }
    if !unresolved.is_empty() {
        println!("Unresolved:");
        for item in unresolved {
            println!(
                "  ? {}  {}  ({})",
                item.source_id,
                item.path.display(),
                item.reason
            );
        }
    }
}

fn print_file(file: &FileResult) {
    let path = file.path.display();
    match &file.outcome {
        FileOutcome::Parsed { achievements } => {
            println!("  {}  ok ({achievements})  {path}", file.source_id);
        }
        FileOutcome::Error { message } => {
            println!("  {}  error: {message}  {path}", file.source_id);
        }
    }
}

fn print_unlock(unlock: &Unlock) {
    if unlock.unlocked {
        let time = unlock
            .unlocked_at
            .and_then(local_time)
            .unwrap_or_else(|| "(no time)".to_owned());
        println!("  + {}  {time}", unlock.api_name);
    } else if let Some(progress) = &unlock.progress {
        println!(
            "  - {}  {}/{}",
            unlock.api_name, progress.current, progress.max
        );
    } else {
        println!("  - {}", unlock.api_name);
    }
}

fn local_time(timestamp: i64) -> Option<String> {
    let utc = DateTime::from_timestamp(timestamp, 0)?;
    Some(
        utc.with_timezone(&Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
    )
}
