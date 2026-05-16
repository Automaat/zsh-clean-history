use std::fs;

#[test]
fn mise_pins_coverage_runner() {
    let config = fs::read_to_string(".mise.toml").unwrap();

    assert!(config.contains(r#"rust = { version = "1.95.0", components = "llvm-tools-preview" }"#));
    assert!(config.contains(r#""cargo:cargo-llvm-cov" = "0.8.6""#));
    assert!(
        config.contains(r#"run = "cargo llvm-cov --all-targets --lcov --output-path lcov.info""#)
    );
}
