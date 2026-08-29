use anyhow::{Context, Result};
use std::time::Instant;
use tockyvoice_lib::terminology::{CompiledVocabulary, VocabularySnapshot};

fn main() -> Result<()> {
    let path = std::env::args()
        .nth(1)
        .context("usage: terminology_perf <snapshot.json>")?;
    let started = Instant::now();
    let bytes = std::fs::read(path).context("reading vocabulary snapshot")?;
    let loaded = started.elapsed();

    let started = Instant::now();
    let snapshot: VocabularySnapshot =
        serde_json::from_slice(&bytes).context("parsing vocabulary snapshot")?;
    let parsed = started.elapsed();
    let entry_count = snapshot.entries.len();

    let started = Instant::now();
    let compiled = CompiledVocabulary::compile(snapshot).context("compiling alias index")?;
    let compiled_time = started.elapsed();

    let started = Instant::now();
    let first = compiled.normalize("PT2 DENSO TQ heo bánh 950");
    let first_normalization = started.elapsed();

    let started = Instant::now();
    for _ in 0..10_000 {
        std::hint::black_box(compiled.normalize("PT2 DENSO TQ heo bánh 950"));
    }
    let repeated = started.elapsed();

    let started = Instant::now();
    let provider_count = compiled.provider_terms()?.len();
    let provider_projection = started.elapsed();
    println!(
        "{{\"entries\":{entry_count},\"provider_terms\":{provider_count},\"sample\":{},\"file_read_ms\":{:.3},\"json_parse_ms\":{:.3},\"alias_compile_ms\":{:.3},\"first_normalization_ms\":{:.3},\"repeated_10000_ms\":{:.3},\"provider_projection_ms\":{:.3}}}",
        serde_json::to_string(&first)?,
        loaded.as_secs_f64() * 1000.0,
        parsed.as_secs_f64() * 1000.0,
        compiled_time.as_secs_f64() * 1000.0,
        first_normalization.as_secs_f64() * 1000.0,
        repeated.as_secs_f64() * 1000.0,
        provider_projection.as_secs_f64() * 1000.0,
    );
    Ok(())
}
