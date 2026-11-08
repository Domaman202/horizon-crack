//! The owner gate from the outside: with no account that owns Minecraft: Java Edition in the
//! launcher, neither `horizon-launcher play` nor the MCP server's play starts the game.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const REFUSED: &str = "Sign in with a Microsoft account that owns Minecraft: Java Edition to play.";

/// Fresh homes with one installed (not prepared) version and an offline name, as a player who
/// played offline before the gate has.
fn home(name: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("horizon-owner-gate-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let version = root.join("launcher/versions/v2026.10.27.01");
    std::fs::create_dir_all(&version).unwrap();
    std::fs::write(version.join(".installed"), "").unwrap();
    std::fs::write(
        root.join("launcher/accounts.json"),
        r#"{"accounts":[],"selected":null,"offline":["Builder_1"],"offline_selected":"Builder_1"}"#,
    )
    .unwrap();
    root
}

fn launcher(root: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_horizon-launcher"));
    cmd.args(args)
        .env("HORIZON_HOME", root.join("game"))
        .env("HORIZON_LAUNCHER_HOME", root.join("launcher"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("HORIZON_LAUNCHER_NO_SELF_UPDATE", "1")
        .env("HORIZON_NO_AUDIO", "1")
        .env("HORIZON_TELEMETRY_URL", "off")
        // Memory only: the test never reads the player's keychain.
        .env("HORIZON_NO_KEYCHAIN", "1")
        .env("HORIZON_MSA_CLIENT_ID", "test-client");
    cmd
}

fn text(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr)
}

