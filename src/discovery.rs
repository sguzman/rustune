use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use tracing::{debug, instrument, trace, warn};

use crate::datfile::dat_path_for_text;
use crate::sources::{SourceSpec, WeightedSource};

const DEFAULT_FORTUNE_PATH: &str = "/usr/share/fortune:/usr/local/share/fortune:/usr/share/games/fortunes:/usr/local/share/games/fortunes";

#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    pub allow_any: bool,
    pub offensive_only: bool,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            allow_any: false,
            offensive_only: false,
        }
    }
}

#[instrument(skip_all)]
pub fn discover_weighted_sources(
    specs: &[SourceSpec],
    config: &DiscoveryConfig,
) -> Result<Vec<WeightedSource>> {
    if config.offensive_only && config.allow_any {
        warn!("both offensive_only and allow_any are set; offensive_only wins");
    }

    let raw_specs = if specs.is_empty() {
        default_source_specs()?
    } else {
        specs.to_vec()
    };
    debug!(input_specs = raw_specs.len(), "running source discovery");

    let mut out = Vec::new();
    for spec in raw_specs {
        let discovered = resolve_spec_paths(&spec.path)?;
        if discovered.is_empty() {
            trace!(path = %spec.path.display(), "no sources discovered for spec");
            continue;
        }
        let share = spec.percent.map(|p| p / (discovered.len() as f64));
        for path in discovered {
            if is_offensive(&path) && !config.allow_any && !config.offensive_only {
                trace!(path = %path.display(), "skipping offensive file in default mode");
                continue;
            }
            if config.offensive_only && !is_offensive(&path) {
                trace!(path = %path.display(), "skipping non-offensive file in offensive-only mode");
                continue;
            }
            out.push(WeightedSource {
                path,
                explicit_percent: share,
            });
        }
    }

    if out.is_empty() {
        bail!("no fortune database files discovered");
    }

    debug!(discovered = out.len(), "source discovery completed");
    Ok(out)
}

fn is_offensive(path: &Path) -> bool {
    path.file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.ends_with("-o"))
        .unwrap_or(false)
}

fn default_source_specs() -> Result<Vec<SourceSpec>> {
    let mut out = Vec::new();
    for dir in default_base_dirs() {
        for locale_dir in locale_candidates_for_dir(&dir) {
            if locale_dir.is_dir() {
                out.push(SourceSpec {
                    path: locale_dir,
                    percent: None,
                });
            }
        }
        if dir.is_dir() {
            out.push(SourceSpec {
                path: dir,
                percent: None,
            });
        }
    }
    if out.is_empty() {
        bail!("no default fortune directories found");
    }
    Ok(out)
}

fn default_base_dirs() -> Vec<PathBuf> {
    let path = env::var("FORTUNE_PATH").unwrap_or_else(|_| DEFAULT_FORTUNE_PATH.to_string());
    path.split(':')
        .filter(|entry| !entry.is_empty())
        .map(PathBuf::from)
        .collect()
}

fn locale_candidates_for_dir(base_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(lang_env) = env::var("LANG") {
        for lang in lang_env.split(':').filter(|x| !x.trim().is_empty()) {
            let normalized = lang.split('.').next().unwrap_or(lang);
            if !normalized.is_empty() {
                out.push(base_dir.join(normalized));
                if let Some((short, _)) = normalized.split_once('_') {
                    out.push(base_dir.join(short));
                } else if normalized.len() > 2 {
                    out.push(base_dir.join(&normalized[0..2]));
                }
            }
        }
    }
    out
}

#[instrument(skip_all, fields(spec = %spec_path.display()))]
fn resolve_spec_paths(spec_path: &Path) -> Result<Vec<PathBuf>> {
    if spec_path == Path::new("all") {
        let mut dedup = BTreeSet::new();
        for dir in default_base_dirs() {
            for entry in collect_fortune_files(&dir)? {
                dedup.insert(entry);
            }
        }
        return Ok(dedup.into_iter().collect());
    }

    if spec_path.is_dir() {
        return collect_fortune_files(spec_path);
    }

    if spec_path.is_file() {
        if dat_path_for_text(spec_path).is_file() {
            return Ok(vec![spec_path.to_path_buf()]);
        }
        bail!(
            "fortune text file '{}' has no .dat sibling",
            spec_path.display()
        );
    }

    if let Some(alt) = offensive_alternate(spec_path) {
        if alt.is_file() && dat_path_for_text(&alt).is_file() {
            return Ok(vec![alt]);
        }
    }

    Ok(Vec::new())
}

fn offensive_alternate(path: &Path) -> Option<PathBuf> {
    let fname = path.file_name()?.to_str()?;
    if let Some(stripped) = fname.strip_suffix("-o") {
        return Some(path.with_file_name(stripped));
    }
    Some(path.with_file_name(format!("{fname}-o")))
}

#[instrument(skip_all, fields(dir = %dir.display()))]
fn collect_fortune_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let entries = fs::read_dir(dir)
        .with_context(|| format!("failed reading fortune directory {}", dir.display()))?;
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.') || name.ends_with(".dat") || name.ends_with(".u8") {
            continue;
        }
        let dat_path = dat_path_for_text(&path);
        if dat_path.is_file() {
            files.push(path);
        }
    }
    files.sort();
    debug!(count = files.len(), "collected fortune files");
    Ok(files)
}
