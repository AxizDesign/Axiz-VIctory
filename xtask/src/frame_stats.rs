//! Offline summary of frame intervals measured by an external capture tool.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use anyhow::{Context, Result, ensure};

const FRAME_BUDGET_MS: f64 = 1000.0 / 30.0;

pub fn run(input: &Path, stutter_ms: f64) -> Result<()> {
    ensure!(
        stutter_ms.is_finite() && stutter_ms > 0.0,
        "invalid stutter threshold"
    );
    let file = File::open(input).with_context(|| format!("opening {}", input.display()))?;
    let mut frames = read_frames(BufReader::new(file))?;
    let budget_misses = frames.iter().filter(|&&ms| ms > FRAME_BUDGET_MS).count();
    let stutters = frames.iter().filter(|&&ms| ms > stutter_ms).count();
    frames.sort_unstable_by(f64::total_cmp);
    let count = frames.len();
    let median = frames[(count - 1) / 2].midpoint(frames[count / 2]);
    println!("Frames: {count}");
    println!("Average FPS: {:.3}", 1000.0 / mean(&frames));
    println!(
        "Median FPS (inverse median interval): {:.3}",
        1000.0 / median
    );
    for (label, denominator) in [("1% low", 100), ("0.1% low", 1000)] {
        if count >= denominator {
            let start = count - count.div_ceil(denominator);
            println!("{label} FPS: {:.3}", 1000.0 / mean(&frames[start..]));
        } else {
            println!("{label} FPS: unavailable (requires {denominator} frames)");
        }
    }
    for (label, per_mille) in [("P95", 950), ("P99", 990), ("P99.9", 999)] {
        println!(
            "{label} frametime: {:.3} ms",
            percentile(&frames, per_mille)
        );
    }
    println!("Frames over 30 FPS budget ({FRAME_BUDGET_MS:.3} ms): {budget_misses}/{count}");
    println!(
        "Stutters (> {stutter_ms:.3} ms): {stutters}/{count} ({:.3}%)",
        100.0 * stutters as f64 / count as f64
    );
    Ok(())
}

fn read_frames(reader: impl BufRead) -> Result<Vec<f64>> {
    let mut lines = reader.lines();
    let header = lines.next().context("missing frametime_ms header")??;
    ensure!(
        header.trim_start_matches('\u{feff}').trim() == "frametime_ms",
        "expected a frametime_ms header"
    );
    let mut frames = Vec::new();
    for (index, line) in lines.enumerate() {
        let number = index + 2;
        let line = line.with_context(|| format!("reading line {number}"))?;
        let ms: f64 = line
            .trim()
            .parse()
            .with_context(|| format!("invalid frametime at line {number}"))?;
        ensure!(
            ms.is_finite() && ms > 0.0 && (1000.0 / ms).is_finite(),
            "invalid frametime at line {number}"
        );
        frames.push(ms);
    }
    ensure!(!frames.is_empty(), "capture has no frame intervals");
    Ok(frames)
}

fn mean(values: &[f64]) -> f64 {
    let mut mean = values[0];
    for (index, &value) in values.iter().enumerate().skip(1) {
        mean += (value - mean) / (index + 1) as f64;
    }
    mean
}

fn percentile(sorted: &[f64], per_mille: u128) -> f64 {
    let rank = (sorted.len() as u128 * per_mille).div_ceil(1000);
    sorted[rank as usize - 1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bom_and_rejects_missing_or_invalid_intervals() {
        assert_eq!(
            read_frames("\u{feff}frametime_ms\r\n16.7\r\n50\r\n".as_bytes()).unwrap(),
            [16.7, 50.0]
        );
        for bad in [
            "",
            "fps\n30",
            "frametime_ms\n",
            "frametime_ms\nNaN",
            "frametime_ms\n0",
            "frametime_ms\n16,7",
        ] {
            assert!(read_frames(bad.as_bytes()).is_err());
        }
    }

    #[test]
    fn nearest_rank_preserves_tail_outliers() {
        let mut frames = vec![20.0; 999];
        frames.push(100.0);
        assert_eq!(percentile(&frames, 990), 20.0);
        assert_eq!(percentile(&frames, 999), 20.0);
        assert_eq!(percentile(&frames, 1000), 100.0);
        assert!((mean(&frames) - 20.08).abs() < 1e-10);
    }
}
