//! Subprocess coverage for CLI help, errors, and exit codes.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn fountain() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fountain"))
}

fn run(args: &[&str]) -> Output {
    fountain().args(args).output().expect("run fountain")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

struct TempInput {
    path: PathBuf,
}

impl TempInput {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "fountain-cli-test-{}-{nonce}.bin",
            std::process::id()
        ));
        fs::write(&path, b"fountain CLI test input").expect("write temporary input");
        Self { path }
    }
}

impl Drop for TempInput {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[test]
fn top_level_help_names_commands_and_workflow() {
    let output = run(&["--help"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Send a file as RaptorQ, Raptor10, or BHP fountain-coded UDP packets"));
    assert!(stdout.contains("send"));
    assert!(stdout.contains("recv"));
    assert!(stdout.contains("Start the receiver first"));
    assert!(stdout.contains("fountain send ./myfile.bin"));
}

#[test]
fn send_help_documents_positionals_defaults_and_controls() {
    let output = run(&["send", "--help"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("<PATH>"));
    assert!(stdout.contains("<HOST:PORT>"));
    assert!(stdout.contains("raptorq"));
    assert!(stdout.contains("raptor10"));
    assert!(stdout.contains("[default: 1400]"));
    assert!(stdout.contains("Use 0 to send source symbols only"));
    assert!(stdout.contains("--delay-ms"));
    assert!(stdout.contains("--progress"));
    assert!(stdout.contains("packet, byte, and decode progress"));
}

#[test]
fn recv_help_documents_progress() {
    let output = run(&["recv", "--help"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--progress"));
    assert!(stdout.contains("packet, byte, and decode progress"));
}

#[test]
fn progress_flag_parses_for_send_and_recv() {
    let send = run(&["send", "unused.bin", "127.0.0.1:7878", "--progress"]);
    assert_eq!(send.status.code(), Some(1));
    assert!(stderr(&send).contains("cannot read input file"));

    let recv = run(&["recv", "unused.bin", "--progress", "--timeout-secs", "0"]);
    assert_eq!(recv.status.code(), Some(2));
    assert!(stderr(&recv).contains("--timeout-secs"));
}

#[test]
fn missing_input_reports_path_without_internal_debug_format() {
    let missing = std::env::temp_dir().join("fountain-file-that-does-not-exist.bin");
    let output = run(&[
        "send",
        missing.to_str().expect("UTF-8 temp path"),
        "127.0.0.1:7878",
    ]);
    assert_eq!(output.status.code(), Some(1));
    let message = stderr(&output);
    assert!(message.contains("error: cannot read input file"));
    assert!(message.contains("No such file or directory"));
    assert!(!message.contains("Custom {"));
}

#[test]
fn unknown_codec_is_an_invalid_input_error() {
    let input = TempInput::new();
    let output = run(&[
        "send",
        input.path.to_str().expect("UTF-8 temp path"),
        "127.0.0.1:7878",
        "--codec",
        "bogus",
    ]);
    assert_eq!(output.status.code(), Some(1));
    let message = stderr(&output);
    assert!(message.contains("invalid --codec value"));
    assert!(message.contains("use raptorq, raptor10, or bhp"));
}

#[test]
fn invalid_destination_explains_the_expected_format() {
    let input = TempInput::new();
    let output = run(&[
        "send",
        input.path.to_str().expect("UTF-8 temp path"),
        "127.0.0.1:notaport",
    ]);
    assert_eq!(output.status.code(), Some(1));
    let message = stderr(&output);
    assert!(message.contains("invalid destination"));
    assert!(message.contains("expected HOST:PORT"));
}

#[test]
fn clap_rejects_zero_sizes_with_usage_exit_code() {
    let symbol = run(&["send", "unused.bin", "127.0.0.1:7878", "--symbol-size", "0"]);
    assert_eq!(symbol.status.code(), Some(2));
    assert!(stderr(&symbol).contains("--symbol-size"));

    let timeout = run(&["recv", "unused.bin", "--timeout-secs", "0"]);
    assert_eq!(timeout.status.code(), Some(2));
    assert!(stderr(&timeout).contains("--timeout-secs"));
}
