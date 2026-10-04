use std::fs;

#[test]
fn mise_pins_coverage_runner() {
    let config = fs::read_to_string(".mise.toml").unwrap();

    let rust = config
        .lines()
        .find(|line| line.starts_with("rust = { version = \""))
        .unwrap();
    assert!(!rust.starts_with("rust = { version = \"\""));
    assert!(rust.contains(r#"components = "llvm-tools-preview""#));
    assert!(config.lines().any(|line| {
        line.starts_with(r#""cargo:cargo-llvm-cov" = ""#) && !line.ends_with(r#"= """#)
    }));
    assert!(
        config.contains(r#"run = "cargo llvm-cov --all-targets --lcov --output-path lcov.info""#)
    );
}
