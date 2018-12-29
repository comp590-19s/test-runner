// For working with JSON
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
use serde_json::{Value};

// For reading test settings file
use std::fs::File;
use std::io::prelude::*;

// For running cargo test
use std::process::Command;
use std::process::Output;

fn main() {
    if std::env::args().len() == 1 {
        eprintln!("Usage: test-runner <path-to-settings.json>");
        std::process::exit(1);
    }

    // Read autograder settings
    let settings_file_path = std::env::args().nth(1).unwrap();
    let settings = read_settings(&settings_file_path);

    let mut results = Results::new();

    // Run through each of the suites
    for suite in settings.suites {
        cargo_test(&mut results, &settings.target, &suite);
    }

    // Print results back in gradescope format
    let serialized = serde_json::to_string(&results).unwrap();
    println!("{}", serialized);
}

fn read_settings(path: &str) -> Settings {
    let settings: Settings;
    if let Ok(mut settings_file) = File::open(path) {
        let mut contents = String::new();
        if let Ok(_) = settings_file.read_to_string(&mut contents) {
            settings = serde_json::from_str(&contents).unwrap();
        } else {
            panic!("Could not read settings file: {}", path);
        }
    } else {
        panic!("Could not open settings file: {}", path);
    }
    settings
}

#[derive(Debug, Deserialize)]
struct Settings {
    target: String,
    suites: Vec<Suite>,
}

#[derive(Debug, Deserialize)]
struct Suite {
    number: String,
    name: String,
    points: f64,
    filter: String,
}

#[derive(Debug,Serialize)]
struct Test {
    number: String,
    name: String,
    score: f64,
    max_score: f64,
    output: String,
}

impl Test {
    fn new(number: String, name: String, score: f64, output: String) -> Test {
        Test {
            number,
            name,
            score,
            output, 
            max_score: 1.0f64,
        }
    }

    fn scale(&self, factor: f64) -> Test {
        Test {
            number: self.number.clone(),
            name: self.name.clone(),
            score: round(self.score * factor),
            max_score: round(self.max_score * factor),
            output: self.output.clone(),
        }
    }
}

fn round(input: f64) -> f64 {
    (input * 100.0).round() / 100.0
}

#[derive(Debug,Serialize)]
struct Results {
    tests: Vec<Test>,
    output: String,
}

impl Results {
    fn new() -> Results {
        Results {
            tests: Vec::new(),
            output: String::from(""),
        }
    }
}

/*
 * Run cargo test in a directory, with a test case search filter, and given points.
 * 
 * Since the number of tests to run is not known, their point value is unknown upfront.
 * As such, we need to run the tests, batch the results, and then apply point values
 * retroactively to each test before adding them to the top-level Results object.
 */
fn cargo_test(results: &mut Results, path: &str, suite: &Suite) {
    let output = Command::new("cargo")
                   .current_dir(path)
                   .args(&[
                       "test", 
                       &suite.filter,
                       "--", // the following args give json output
                       "-Z",
                       "unstable-options",
                       "--format=json"
                   ])
                   .env("RUN_TEST_TASKS", "1") // run serially for consistency
                   .output();

    match output {
        Ok(output) => {
            let mut batch: Vec<Test> = Vec::new();
            let mut count = 1;
            for line in output_to_json(&output) {
                let number = suite.number.clone() + "." + &count.to_string();
                if let Some(test) = filter_test_output(&line, &number, &suite.name) {
                    batch.push(test);
                    count += 1;
                }
            }
            if batch.len() > 0 {
                // Allocate points
                let value_scale = suite.points / (batch.len() as f64);
                for test in batch {
                    results.tests.push(test.scale(value_scale));
                }
            }
        },
        Err(err) => {
            results.output = format!("{}", err);
        }
    }
}

fn output_to_json(output: &Output) -> Vec<serde_json::Value> {
    let mut json = Vec::new();
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.split("\n") {
        let v = serde_json::from_str::<Value>(line);
        if let Ok(v) = v {
            json.push(v);
        }
    }
    json
}

fn filter_test_output(line: &serde_json::Value, number: &str, prefix: &str) -> Option<Test> {
    if line["type"] == "test" && line["event"] != "started" {
        // noop
    } else {
        return None;
    }

    let passed = line["event"] == "ok";
    let score = if passed { 1.0 } else { 0.0 };
    let name = prefix.to_owned() + " - " + &line["name"].to_string().replace("\"", "");
    let output = if passed { String::from("") } else { unescape(&line["stdout"].to_string()) };
    Some(Test::new(number.to_string(), name, score, output))
}

/*
 * The following function is sourced from:
 * http://softwaremaniacs.org/blog/2015/05/28/ijson-in-rust-unescape/en/
 */
fn unescape(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        result.push(
            if ch != '\\' {
                ch
            } else {
                match chars.next() {
                    Some('u') => {
                        let value = chars.by_ref().take(4).fold(0, |acc, c| acc * 16 + c.to_digit(16).unwrap());
                        std::char::from_u32(value).unwrap()
                    }
                    Some('b') => '\x08',
                    Some('f') => '\x0c',
                    Some('n') => '\n',
                    Some('r') => '\r',
                    Some('t') => '\t',
                    Some(ch) => ch,
                    _ => panic!("Malformed escape"),
                }
            }
            )
    }
    result
}
