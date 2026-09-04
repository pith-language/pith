//! Import-resolution benchmark: run with
//! `cargo run --release -p pith-loader --example bench_imports`.

use std::time::Instant;

use pith_diag::SourceId;
use pith_loader::{ImportEnv, ModuleSource, load_module};

fn load_ok(text: &str) -> pith_loader::LoadedModule {
    match load_module(
        &ModuleSource::new("bench", SourceId::from_raw(1), "bench.pi", text),
        &ImportEnv::new(),
    ) {
        Ok(loaded) => loaded,
        Err(diagnostics) => {
            assert!(diagnostics.is_empty(), "module did not elaborate");
            unreachable!()
        }
    }
}

fn timed(label: &str, consumer: &str, imports: &ImportEnv) {
    let iterations = 50;
    // Warm up once and make sure it actually elaborates.
    load_module(
        &ModuleSource::new("consumer", SourceId::from_raw(2), "consumer.pi", consumer),
        imports,
    )
    .unwrap_or_else(|_| unreachable!("{label} did not elaborate"));

    let start = Instant::now();
    for _ in 0..iterations {
        let source = ModuleSource::new("consumer", SourceId::from_raw(2), "consumer.pi", consumer);
        let result = load_module(&source, imports);
        assert!(result.is_ok());
    }
    let elapsed = start.elapsed();
    println!(
        "{label:<40} {:>9.1} us/load",
        elapsed.as_micros() as f64 / iterations as f64
    );
}

fn main() {
    for &decls in &[200_usize, 2_000, 5_000] {
        // Dependency: `decls` documented declarations.
        let mut dependency = String::new();
        for index in 0..decls {
            dependency.push_str(&format!("-- doc {index}\nnominal A{index} = Text\n"));
        }
        let loaded = load_ok(&dependency);

        // Consumer: one qualified reference per dependency declaration.
        let mut consumer = String::from("import bench\n");
        for index in 0..decls {
            consumer.push_str(&format!("nominal W{index} = bench.A{index}\n"));
        }

        let mut imports = ImportEnv::new();
        imports.insert_loaded(&loaded);
        timed(
            &format!("{decls} decls, {decls} qualified refs"),
            &consumer,
            &imports,
        );
    }
}
