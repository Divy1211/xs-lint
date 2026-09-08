use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new(source: &str) -> Self {
        let directory = loop {
            let id = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("xs-check-exit-status-{}-{id}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => break path,
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(err) => panic!("could not create test directory: {err}"),
            }
        };
        fs::write(directory.join("test.xs"), source).unwrap();
        Self(directory)
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_xs-check"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn valid_input_and_informational_invocations_succeed() {
    let fixture = Fixture::new("void main() {}");
    for args in [&["test.xs"][..], &["--help"], &["--version"], &[]] {
        assert!(fixture.run(args).status.success(), "{args:?}");
    }
}

#[test]
fn syntax_and_type_errors_fail() {
    for source in ["void main( {", "void main() { int count = \"invalid\"; }"] {
        let fixture = Fixture::new(source);
        assert_eq!(fixture.run(&["test.xs"]).status.code(), Some(1), "{source}");
    }
}

#[test]
fn missing_input_and_include_fail() {
    let fixture = Fixture::new("include \"missing.xs\";");
    assert_eq!(fixture.run(&["missing.xs"]).status.code(), Some(1));
    assert_eq!(fixture.run(&["test.xs"]).status.code(), Some(1));
}

#[test]
fn missing_or_invalid_extra_prelude_fails() {
    let fixture = Fixture::new("void main() {}");
    fs::write(fixture.0.join("invalid.xs"), "int value = \"invalid\";").unwrap();
    for path in ["missing.xs", "invalid.xs"] {
        assert_eq!(
            fixture
                .run(&["test.xs", "--extra-prelude-path", path])
                .status
                .code(),
            Some(1)
        );
    }
}

#[test]
fn warnings_remain_successful_including_when_ignored() {
    let fixture = Fixture::new("string text = \"warning\";");
    let output = fixture.run(&["test.xs"]);
    assert!(String::from_utf8_lossy(&output.stdout).contains("TopStrInit"));
    assert!(output.status.success());
    assert!(fixture
        .run(&["test.xs", "--ignores", "TopStrInit"])
        .status
        .success());
}
