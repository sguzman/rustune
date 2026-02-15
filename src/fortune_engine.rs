use anyhow::{Result, bail};
use regex::Regex;
use tracing::{debug, instrument, trace, warn};

use crate::datfile::{FortuneFile, LengthFilter};
use crate::rng::FortuneRng;
use crate::sources::WeightedSource;

#[derive(Debug, Clone)]
pub struct LoadedSource {
    pub db: FortuneFile,
    pub explicit_percent: Option<f64>,
    pub candidate_indices: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct FortuneSelection {
    pub source_path: std::path::PathBuf,
    pub record_index: usize,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct MatchRecord {
    pub source_path: std::path::PathBuf,
    pub record_index: usize,
    pub text: String,
}

#[instrument(skip_all)]
pub fn load_sources(
    discovered: &[WeightedSource],
    length_filter: LengthFilter,
) -> Result<Vec<LoadedSource>> {
    let mut out = Vec::new();
    for source in discovered {
        let db = FortuneFile::open(&source.path)?;
        let candidate_indices = db.candidate_indices(length_filter)?;
        if candidate_indices.is_empty() {
            trace!(path = %db.text_path.display(), "source has zero candidates under length filter");
            continue;
        }
        out.push(LoadedSource {
            db,
            explicit_percent: source.explicit_percent,
            candidate_indices,
        });
    }
    if out.is_empty() {
        bail!("no fortune records are available after filtering");
    }
    debug!(loaded = out.len(), "loaded fortune sources");
    Ok(out)
}

#[instrument(skip_all)]
pub fn calculate_probabilities(entries: &[LoadedSource], equal_prob: bool) -> Result<Vec<f64>> {
    if entries.is_empty() {
        bail!("no source entries were provided");
    }

    let specified_total: f64 = entries.iter().filter_map(|e| e.explicit_percent).sum();
    if specified_total > 100.0 + f64::EPSILON {
        bail!("specified probability total exceeds 100%");
    }
    let remaining = (100.0 - specified_total).max(0.0);

    let base_weights: Vec<f64> = entries
        .iter()
        .map(|entry| {
            if entry.explicit_percent.is_some() {
                0.0
            } else if equal_prob {
                1.0
            } else {
                entry.candidate_indices.len() as f64
            }
        })
        .collect();
    let total_base: f64 = base_weights.iter().sum();

    let mut probs = Vec::with_capacity(entries.len());
    for (entry, base_weight) in entries.iter().zip(&base_weights) {
        let prob = if let Some(p) = entry.explicit_percent {
            p
        } else if total_base > 0.0 {
            remaining * (base_weight / total_base)
        } else {
            0.0
        };
        probs.push(prob.max(0.0));
    }

    let sum: f64 = probs.iter().sum();
    if sum <= 0.0 {
        bail!("computed source probabilities are all zero");
    }
    debug!(sum, count = probs.len(), "calculated source probabilities");
    Ok(probs)
}

#[instrument(skip_all)]
pub fn select_random_fortune(
    entries: &[LoadedSource],
    probabilities: &[f64],
    rng: &mut FortuneRng,
) -> Result<FortuneSelection> {
    if entries.len() != probabilities.len() {
        bail!("entries/probabilities length mismatch");
    }

    let total: f64 = probabilities.iter().sum();
    if total <= 0.0 {
        bail!("total probability is zero");
    }

    let mut marker = rng.next_unit_f64() * total;
    let mut chosen_idx = entries.len() - 1;
    for (idx, probability) in probabilities.iter().enumerate() {
        if marker < *probability {
            chosen_idx = idx;
            break;
        }
        marker -= *probability;
    }

    let chosen = &entries[chosen_idx];
    let record_pos = rng.next_index(chosen.candidate_indices.len());
    let record_index = chosen.candidate_indices[record_pos];
    let text = chosen.db.record_text_lossy(record_index)?;
    debug!(
        source = %chosen.db.text_path.display(),
        record_index,
        "selected random fortune"
    );
    Ok(FortuneSelection {
        source_path: chosen.db.text_path.clone(),
        record_index,
        text,
    })
}

#[instrument(skip_all)]
pub fn collect_matches(entries: &[LoadedSource], regex: &Regex) -> Result<Vec<MatchRecord>> {
    let mut out = Vec::new();
    for source in entries {
        let mut source_matches = 0usize;
        for record_index in &source.candidate_indices {
            let text = source.db.record_text_lossy(*record_index)?;
            if regex.is_match(&text) {
                out.push(MatchRecord {
                    source_path: source.db.text_path.clone(),
                    record_index: *record_index,
                    text,
                });
                source_matches += 1;
            }
        }
        if source_matches == 0 {
            trace!(
                source = %source.db.text_path.display(),
                "source had no regex matches"
            );
        } else {
            warn!(
                source = %source.db.text_path.display(),
                matches = source_matches,
                "source produced regex matches"
            );
        }
    }
    Ok(out)
}
