use std::env;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Result, bail};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use tracing::{debug, instrument, trace};

#[derive(Debug)]
enum Mode {
    HardCoded { values: Vec<u64>, index: usize },
    Seeded(StdRng),
    Thread,
}

#[derive(Debug)]
pub struct FortuneRng {
    mode: Mode,
}

impl FortuneRng {
    #[instrument]
    pub fn from_env() -> Result<Self> {
        if let Ok(raw) = env::var("FORTUNE_MOD_RAND_HARD_CODED_VALS") {
            let values = parse_hardcoded_values(&raw)?;
            if !values.is_empty() {
                debug!(values = values.len(), "using hard coded RNG sequence");
                return Ok(Self {
                    mode: Mode::HardCoded { values, index: 0 },
                });
            }
        }

        if env_truthy("FORTUNE_MOD_USE_SRAND") {
            let secs = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let seed = secs ^ (std::process::id() as u64);
            debug!(seed, "using srand-compatible seeded RNG mode");
            return Ok(Self {
                mode: Mode::Seeded(StdRng::seed_from_u64(seed)),
            });
        }

        debug!("using thread RNG mode");
        Ok(Self { mode: Mode::Thread })
    }

    pub fn next_u64(&mut self) -> u64 {
        match &mut self.mode {
            Mode::HardCoded { values, index } => {
                let value = values[*index % values.len()];
                *index += 1;
                trace!(value, pos = *index, "hard-coded RNG yielded value");
                value
            }
            Mode::Seeded(rng) => rng.random::<u64>(),
            Mode::Thread => rand::random::<u64>(),
        }
    }

    pub fn next_index(&mut self, upper: usize) -> usize {
        if upper == 0 {
            return 0;
        }
        (self.next_u64() as usize) % upper
    }

    pub fn next_unit_f64(&mut self) -> f64 {
        let raw = self.next_u64();
        (raw as f64) / ((u64::MAX as f64) + 1.0)
    }
}

fn parse_hardcoded_values(raw: &str) -> Result<Vec<u64>> {
    let mut out = Vec::new();
    for token in raw
        .split(|c: char| c == ',' || c == ';' || c.is_ascii_whitespace())
        .filter(|x| !x.is_empty())
    {
        let parsed = token
            .parse::<u64>()
            .map_err(|_| anyhow::anyhow!("invalid hardcoded RNG value '{token}'"))?;
        out.push(parsed);
    }
    if out.is_empty() {
        bail!("FORTUNE_MOD_RAND_HARD_CODED_VALS is set but contains no numeric values");
    }
    Ok(out)
}

fn env_truthy(name: &str) -> bool {
    env::var(name)
        .ok()
        .map(|v| !matches!(v.as_str(), "" | "0" | "false" | "False" | "FALSE"))
        .unwrap_or(false)
}
