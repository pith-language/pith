//! Load-time benchmark: run with
//! `cargo run --release -p pith-loader --example bench_load`.

use std::time::Instant;

use pith_diag::SourceId;
use pith_loader::{ImportEnv, ModuleSource, load_module};

fn timed(label: &str, text: &str, iterations: usize) {
    // Warm up once and make sure it actually elaborates.
    let first = load_module(
        &ModuleSource::new("bench", SourceId::from_raw(1), "bench.pi", text),
        &ImportEnv::new(),
    );
    assert!(first.is_ok(), "{label} did not elaborate");

    let start = Instant::now();
    for _ in 0..iterations {
        let source = ModuleSource::new("bench", SourceId::from_raw(1), "bench.pi", text);
        let result = load_module(&source, &ImportEnv::new());
        assert!(result.is_ok());
    }
    let elapsed = start.elapsed();
    println!(
        "{label:<28} {:>9.1} us/load",
        elapsed.as_micros() as f64 / iterations as f64
    );
}

fn main() {
    let xylem = include_str!("../../xylem/xylem.pi");
    timed("xylem.pi (realistic)", xylem, 500);

    // A medium synthetic module: a mix of aliases, records, and sums forming
    // a chain, plus rules that reference them.
    let mut medium = String::new();
    medium.push_str("nominal Base = Text\n");
    for index in 0u32..300 {
        if index == 0 {
            medium.push_str("type Alias0 = Base\n");
        } else {
            medium.push_str(&format!(
                "type Alias{index} = Alias{}\n",
                index.saturating_sub(1)
            ));
        }
        medium.push_str(&format!(
            "nominal Rec{index} = {{a: Alias{index}, b: List<Alias{index}>}}\n"
        ));
        medium.push_str(&format!("sum Sum{index} = none | some(Rec{index})\n"));
        if index % 10 == 0 {
            medium.push_str(&format!(
                "pure rule rule{index}(Sum{index}) -> Rec{index} = host\n"
            ));
        }
    }
    timed("medium synthetic (~1200 decls)", &medium, 200);

    // The pathological deep chain.
    let mut deep = String::from("nominal Base = Text\ntype Link0 = Base\n");
    for index in 1..8_000_u32 {
        deep.push_str(&format!(
            "type Link{index} = Link{}\n",
            index.saturating_sub(1)
        ));
    }
    timed("deep chain (8000 aliases)", &deep, 20);
}
