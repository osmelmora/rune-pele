//! The `pele` binary's gate and command line, driven as a user drives them.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn pele_with(args: &[&str], env: &[(&str, String)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pele"));
    command.args(args).current_dir(root());
    for (name, value) in env {
        command.env(name, value);
    }
    command.output().expect("the pele binary runs")
}

fn pele(args: &[&str]) -> Output {
    pele_with(args, &[])
}

fn status(output: &Output) -> i32 {
    output.status.code().expect("pele exits with a status")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// An empty folder of this test's own.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli").join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch folder");
    dir
}

/// A copy of the pinned corpus to break.
fn corpus_copy(dir: &Path) -> PathBuf {
    let corpus = dir.join("corpus");
    fs::create_dir_all(&corpus).expect("corpus folder");
    for entry in fs::read_dir(pele_oracle::corpus_dir()).expect("mise run setup has run") {
        let path = entry.expect("corpus entry").path();
        fs::copy(&path, corpus.join(path.file_name().expect("a file name"))).expect("copy a corpus file");
    }
    corpus
}

fn path(path: &Path) -> &str {
    path.to_str().expect("a UTF-8 path")
}

fn parity(corpus: &Path, out: &Path, extra: &[&str]) -> Output {
    let mut args = vec!["parity", "--corpus", path(corpus), "--out", path(out)];
    args.extend_from_slice(extra);
    pele(&args)
}

fn total_row(output: &Output) -> Option<String> {
    stdout(output)
        .lines()
        .find(|line| line.starts_with("total"))
        .map(|line| line.split_whitespace().skip(1).collect::<Vec<_>>().join(" "))
}

#[test]
fn the_unbroken_corpus_passes_with_every_source_identical() {
    let dir = scratch("unbroken");
    let corpus = corpus_copy(&dir);
    let sources: usize = pele_oracle::load_corpus(&corpus).expect("the copy loads").iter().map(|(_, s)| s.len()).sum();
    let run = parity(&corpus, &dir.join("out"), &[]);
    assert_eq!(status(&run), 0, "{}", stderr(&run));
    assert_eq!(total_row(&run), Some(format!("{sources} {sources} 0 0 0")));
}

#[test]
fn a_corpus_file_that_is_not_json_fails_the_run_and_is_named() {
    let dir = scratch("malformed");
    let corpus = corpus_copy(&dir);
    fs::write(corpus.join("pie-docs.json"), "{not json").unwrap();
    let run = parity(&corpus, &dir.join("out"), &[]);
    assert_eq!(status(&run), 66, "{}", stderr(&run));
    assert!(stderr(&run).contains("pie-docs.json: not a JSON array of sources"), "{}", stderr(&run));
    assert_eq!(total_row(&run), None);
}

#[test]
fn a_missing_corpus_file_fails_the_run_and_is_named() {
    let dir = scratch("missing");
    let corpus = corpus_copy(&dir);
    fs::remove_file(corpus.join("pie-docs.json")).unwrap();
    let run = parity(&corpus, &dir.join("out"), &[]);
    assert_eq!(status(&run), 66, "{}", stderr(&run));
    assert!(stderr(&run).contains("pie-docs.json: No such file"), "{}", stderr(&run));
}

#[test]
fn a_corpus_file_with_no_sources_fails_the_run_and_is_named() {
    let dir = scratch("empty");
    let corpus = corpus_copy(&dir);
    fs::write(corpus.join("pie-docs.json"), "[]").unwrap();
    for extra in [&[][..], &["--type", "pie"][..]] {
        let run = parity(&corpus, &dir.join("out"), extra);
        assert_eq!(status(&run), 66, "{extra:?}: {}", stderr(&run));
        assert!(stderr(&run).contains("pie-docs.json: holds no sources"), "{extra:?}: {}", stderr(&run));
    }
}

#[test]
fn a_source_pele_rejects_fails_the_run_unless_the_run_expects_it() {
    let dir = scratch("rejected");
    let corpus = corpus_copy(&dir);
    let pie: Vec<String> = serde_json::from_slice(&fs::read(corpus.join("pie-docs.json")).unwrap()).unwrap();
    let broken = serde_json::json!([pie[0], "no diagram here"]).to_string();
    fs::write(corpus.join("pie-docs.json"), broken).unwrap();
    let run = parity(&corpus, &dir.join("out"), &["--type", "pie"]);
    assert_eq!(status(&run), 1, "{}", stderr(&run));
    assert_eq!(total_row(&run).as_deref(), Some("2 1 0 0 1"));
    assert!(stderr(&run).contains("pie 1: Pele rejects the source"), "{}", stderr(&run));
    assert!(stderr(&run).contains("rejected by Pele: 1, expected 0"), "{}", stderr(&run));
    let expected = parity(&corpus, &dir.join("out"), &["--type", "pie", "--expect-pele-errors", "1"]);
    assert_eq!(status(&expected), 0, "{}", stderr(&expected));
    let wrong_count = parity(&corpus, &dir.join("out"), &["--type", "pie", "--expect-pele-errors", "2"]);
    assert_eq!(status(&wrong_count), 1, "{}", stderr(&wrong_count));
}

#[test]
fn a_run_that_compares_nothing_fails() {
    let dir = scratch("nothing");
    let corpus = corpus_copy(&dir);
    fs::write(corpus.join("pie-docs.json"), r#"["no diagram here"]"#).unwrap();
    let run = parity(&corpus, &dir.join("out"), &["--type", "pie", "--expect-pele-errors", "1"]);
    assert_eq!(status(&run), 1, "{}", stderr(&run));
    assert_eq!(total_row(&run).as_deref(), Some("1 0 0 0 1"));
    assert!(stderr(&run).contains("compared: 0"), "{}", stderr(&run));
}

#[test]
fn a_reused_out_folder_keeps_nothing_from_the_earlier_run() {
    let dir = scratch("reused");
    let corpus = corpus_copy(&dir);
    let out = dir.join("out");
    assert_eq!(status(&parity(&corpus, &out, &["--type", "pie"])), 0);
    assert!(out.join("pie/1.ours.svg").is_file(), "the first run writes both pie sources");
    fs::write(corpus.join("pie-docs.json"), r#"["no diagram here"]"#).unwrap();
    parity(&corpus, &out, &["--type", "pie", "--expect-pele-errors", "1"]);
    let mut left: Vec<String> =
        fs::read_dir(out.join("pie")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    left.sort();
    assert_eq!(left, ["0.mmd"]);
    assert_eq!(fs::read_to_string(out.join("pie/0.mmd")).unwrap(), "no diagram here");
}

#[test]
fn an_out_folder_pele_parity_did_not_write_is_left_alone() {
    let dir = scratch("foreign");
    let corpus = corpus_copy(&dir);
    let out = dir.join("out");
    fs::create_dir_all(&out).unwrap();
    fs::write(out.join("keep.txt"), "mine").unwrap();
    let run = parity(&corpus, &out, &["--type", "pie"]);
    assert_eq!(status(&run), 73, "{}", stderr(&run));
    assert!(stderr(&run).contains("did not write it"), "{}", stderr(&run));
    assert_eq!(fs::read_to_string(out.join("keep.txt")).unwrap(), "mine");
    assert!(!out.join("pie").exists());
}

#[test]
fn an_out_path_that_cannot_be_a_folder_is_an_io_error() {
    let dir = scratch("out-file");
    let corpus = corpus_copy(&dir);
    let file = dir.join("taken");
    fs::write(&file, "a file").unwrap();
    for out in [file.as_path(), Path::new("/dev/null")] {
        let run = parity(&corpus, out, &["--type", "pie"]);
        assert_eq!(status(&run), 73, "{}: {}", out.display(), stderr(&run));
        assert!(stderr(&run).contains("Not a directory"), "{}: {}", out.display(), stderr(&run));
    }
    assert_eq!(fs::read_to_string(&file).unwrap(), "a file");
}

#[test]
fn unknown_options_and_options_without_values_are_usage_errors() {
    let fixture = "tests/fixtures/pie-pets.mmd";
    let cases: [&[&str]; 9] = [
        &["render", fixture, "--engine"],
        &["render", fixture, "-o"],
        &["render", fixture, "--bogus"],
        &["render", fixture, "-o", "--engine", "pele"],
        &["render", fixture, fixture],
        &["parity", "--bogus"],
        &["parity", "--type"],
        &["parity", "--out"],
        &["parity", "--expect-pele-errors", "some"],
    ];
    for args in cases {
        let run = pele(args);
        assert_eq!(status(&run), 64, "{args:?}: {}", stderr(&run));
        assert!(stdout(&run).is_empty(), "{args:?} prints nothing on stdout");
        assert!(stderr(&run).contains("usage:"), "{args:?}: {}", stderr(&run));
    }
}

/// A Node that answers `--version` with `version` and fails at anything else.
fn fake_node(dir: &Path, version: &str) -> PathBuf {
    let bin = dir.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let node = bin.join("node");
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo {version}; exit 0; fi\necho fake node ran >&2\nexit 1\n"
    );
    fs::write(&node, script).unwrap();
    let mut permissions = fs::metadata(&node).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
    fs::set_permissions(&node, permissions).unwrap();
    bin
}

fn pinned_node() -> String {
    let toml = fs::read_to_string(root().join("mise.toml")).unwrap();
    let pin =
        toml.lines().find_map(|line| line.strip_prefix("node = \"")?.strip_suffix('"')).expect("mise.toml pins node");
    format!("v{pin}")
}

#[test]
fn the_oracle_runs_the_node_mise_names_not_the_first_on_path() {
    let dir = scratch("path-node");
    let shadow = fake_node(&dir, &pinned_node());
    let path = format!("{}:{}", shadow.display(), std::env::var("PATH").unwrap());
    let run = pele_with(&["render", "tests/fixtures/pie-pets.mmd", "--engine", "pele"], &[("PATH", path)]);
    assert_eq!(status(&run), 0, "{}", stderr(&run));
    assert!(stdout(&run).starts_with("<svg"), "{}", stdout(&run));
}

#[test]
fn the_oracle_refuses_a_node_other_than_the_pinned_version_and_names_both() {
    let dir = scratch("other-node");
    fake_node(&dir, "v0.0.1");
    let env = [("MISE_NODE_VERSION", format!("path:{}", dir.display()))];
    let run = pele_with(&["render", "tests/fixtures/pie-pets.mmd", "--engine", "pele"], &env);
    assert_eq!(status(&run), 70, "{}", stderr(&run));
    let message = stderr(&run);
    assert!(
        message.contains("is v0.0.1") && message.contains(&format!("mise.toml pins {}", pinned_node())),
        "{message}"
    );
}

#[test]
fn an_oracle_that_crashes_is_an_error_with_status_70_in_render_and_parity() {
    let dir = scratch("crash");
    fake_node(&dir, &pinned_node());
    let env = [("MISE_NODE_VERSION", format!("path:{}", dir.display()))];
    let render = pele_with(&["render", "tests/fixtures/pie-pets.mmd", "--engine", "pele"], &env);
    assert_eq!(status(&render), 70, "{}", stderr(&render));
    assert!(stderr(&render).contains("oracle: the oracle failed"), "{}", stderr(&render));
    let out = dir.join("out");
    let parity = pele_with(&["parity", "--type", "pie", "--out", path(&out)], &env);
    assert_eq!(status(&parity), 70, "{}", stderr(&parity));
    assert!(stderr(&parity).contains("oracle: the oracle failed"), "{}", stderr(&parity));
}
