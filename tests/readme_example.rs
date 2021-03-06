use pretty_assertions::assert_eq;
use std::fs::File;
use std::io::Read;
use std::process::Command;

#[test]
fn readme_example() {
    let expected = extract_example_from_module_docs("src/lib.rs");
    let actual = run_example("readme");
    assert_eq!(normalize_output(&expected), normalize_output(&actual));
}

fn normalize_output(output: &str) -> Vec<String> {
    output
        .lines()
        .map(|line| {
            // Remove duration and timing because it's random
            line[..59].to_string()
        })
        .collect::<Vec<_>>()
}

fn extract_example_from_module_docs(file_path: &str) -> String {
    let mut file = File::open(file_path).unwrap();
    let mut buf = String::new();
    file.read_to_string(&mut buf).unwrap();

    let mut example = Vec::new();
    let mut found_start = false;
    for line in buf.lines() {
        if !found_start {
            if line == "//! ```text" {
                found_start = true;
            }
        } else {
            if line == "//! ```" {
                break;
            }

            example.push(line.strip_prefix("//! ").unwrap());
        }
    }

    example.join("\n")
}

fn run_example(example_name: &str) -> String {
    let out = Command::new("cargo")
        .arg("run")
        .arg("--quiet")
        .arg("--example")
        .arg(example_name)
        .env("TERM", "dumb")
        .output()
        .unwrap();
    if !out.status.success() {
        panic!("command failed with code {}", out.status);
    }

    String::from_utf8(out.stdout).unwrap()
}
