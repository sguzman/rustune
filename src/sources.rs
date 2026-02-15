use std::path::PathBuf;

use anyhow::{Result, bail};
use tracing::{debug, instrument};

#[derive(Debug, Clone, PartialEq)]
pub struct SourceSpec {
    pub path: PathBuf,
    pub percent: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WeightedSource {
    pub path: PathBuf,
    pub explicit_percent: Option<f64>,
}

#[instrument(skip_all)]
pub fn parse_source_specs(args: &[String]) -> Result<Vec<SourceSpec>> {
    let mut parsed = Vec::new();
    let mut i = 0_usize;
    while i < args.len() {
        let token = &args[i];
        if let Some((pct, path_part)) = parse_percent_prefix(token)? {
            if path_part.is_empty() {
                let next = args.get(i + 1).ok_or_else(|| {
                    anyhow::anyhow!("missing path after percentage token '{token}'")
                })?;
                parsed.push(SourceSpec {
                    path: PathBuf::from(next),
                    percent: Some(pct),
                });
                i += 2;
                continue;
            }
            parsed.push(SourceSpec {
                path: PathBuf::from(path_part),
                percent: Some(pct),
            });
        } else {
            parsed.push(SourceSpec {
                path: PathBuf::from(token),
                percent: None,
            });
        }
        i += 1;
    }

    let total_specified: f64 = parsed.iter().filter_map(|p| p.percent).sum();
    if total_specified > 100.0 + f64::EPSILON {
        bail!("specified percentages exceed 100% (got {total_specified:.3}%)");
    }

    debug!(count = parsed.len(), total_specified, "parsed source specs");
    Ok(parsed)
}

fn parse_percent_prefix(token: &str) -> Result<Option<(f64, &str)>> {
    if let Some(idx) = token.find('%') {
        let (lhs, rhs) = token.split_at(idx);
        if lhs.is_empty() {
            return Ok(None);
        }
        if lhs.chars().all(|c| c.is_ascii_digit() || c == '.') {
            let pct: f64 = lhs
                .parse()
                .map_err(|_| anyhow::anyhow!("invalid percentage value '{lhs}'"))?;
            if !(0.0..=100.0).contains(&pct) {
                bail!("percentage out of range 0..=100: {pct}");
            }
            return Ok(Some((pct, rhs.trim_start_matches('%'))));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_inline_percent() {
        let args = vec!["10%foo".to_string(), "bar".to_string()];
        let parsed = parse_source_specs(&args).expect("parse");
        assert_eq!(parsed[0].percent, Some(10.0));
        assert_eq!(parsed[0].path, PathBuf::from("foo"));
        assert_eq!(parsed[1].percent, None);
    }

    #[test]
    fn parse_split_percent() {
        let args = vec!["25%".to_string(), "foo".to_string()];
        let parsed = parse_source_specs(&args).expect("parse");
        assert_eq!(parsed[0].percent, Some(25.0));
        assert_eq!(parsed[0].path, PathBuf::from("foo"));
    }
}
