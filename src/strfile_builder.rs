use anyhow::{Result, bail};
use tracing::{debug, instrument};

use crate::datfile::{DatFile, DatHeader, RecordSpan, STR_ORDERED, STR_RANDOM, STRFILE_VERSION};
use crate::rng::FortuneRng;

#[derive(Debug, Clone, Copy)]
pub struct BuildOptions {
    pub delimiter: u8,
    pub randomize_offsets: bool,
    pub order_offsets: bool,
    pub allow_empty: bool,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            delimiter: b'%',
            randomize_offsets: false,
            order_offsets: false,
            allow_empty: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildStats {
    pub record_count: usize,
    pub shortest_record: usize,
    pub longest_record: usize,
}

#[instrument(skip_all)]
pub fn parse_record_spans(input: &[u8], delimiter: u8, allow_empty: bool) -> Vec<RecordSpan> {
    let mut out = Vec::new();
    let mut cursor = 0_usize;
    let mut start = 0_usize;

    while cursor < input.len() {
        let line_end = find_next_newline(input, cursor).unwrap_or(input.len());
        let mut content_end = line_end;
        if content_end > cursor && input[content_end - 1] == b'\r' {
            content_end -= 1;
        }

        let is_delim_line = (content_end - cursor == 1) && (input[cursor] == delimiter);
        if is_delim_line {
            if allow_empty || cursor > start {
                out.push(RecordSpan { start, end: cursor });
            }
            start = if line_end < input.len() {
                line_end + 1
            } else {
                line_end
            };
        }

        cursor = if line_end < input.len() {
            line_end + 1
        } else {
            line_end
        };
    }

    if allow_empty || start < input.len() {
        out.push(RecordSpan {
            start,
            end: input.len(),
        });
    }

    out
}

#[instrument(skip_all)]
pub fn build_dat_from_text(input: &[u8], opts: BuildOptions) -> Result<(DatFile, BuildStats)> {
    if opts.order_offsets && opts.randomize_offsets {
        bail!("--order and --random cannot be used together");
    }

    let spans = parse_record_spans(input, opts.delimiter, opts.allow_empty);
    if spans.is_empty() {
        bail!("no fortune records were parsed");
    }

    let shortest = spans.iter().map(len_for_span).min().unwrap_or(0);
    let longest = spans.iter().map(len_for_span).max().unwrap_or(0);

    let mut ordered = spans.clone();
    if opts.order_offsets {
        ordered.sort_by(|a, b| input[a.start..a.end].cmp(&input[b.start..b.end]));
    } else if opts.randomize_offsets {
        let mut rng = FortuneRng::from_env()?;
        fisher_yates_shuffle(&mut ordered, &mut rng);
    }

    let mut offsets = Vec::with_capacity(ordered.len());
    for span in &ordered {
        let start = u32::try_from(span.start).map_err(|_| {
            anyhow::anyhow!("record start offset {} exceeds STRFILE u32", span.start)
        })?;
        offsets.push(start);
    }

    let flags = match (opts.randomize_offsets, opts.order_offsets) {
        (true, _) => STR_RANDOM,
        (_, true) => STR_ORDERED,
        _ => 0,
    };

    let header = DatHeader {
        version: STRFILE_VERSION,
        numstr: offsets.len() as u32,
        longlen: longest as u32,
        shortlen: shortest as u32,
        flags,
        delim: opts.delimiter,
    };

    let dat = DatFile { header, offsets };
    let stats = BuildStats {
        record_count: spans.len(),
        shortest_record: shortest,
        longest_record: longest,
    };
    debug!(?stats, flags, "built dat from text");
    Ok((dat, stats))
}

fn fisher_yates_shuffle(items: &mut [RecordSpan], rng: &mut FortuneRng) {
    if items.len() < 2 {
        return;
    }
    for i in (1..items.len()).rev() {
        let j = rng.next_index(i + 1);
        items.swap(i, j);
    }
}

fn len_for_span(span: &RecordSpan) -> usize {
    span.end.saturating_sub(span.start)
}

fn find_next_newline(input: &[u8], start: usize) -> Option<usize> {
    input[start..]
        .iter()
        .position(|b| *b == b'\n')
        .map(|rel| start + rel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_records() {
        let text = b"one\n%\ntwo\n%\nthree\n";
        let spans = parse_record_spans(text, b'%', false);
        assert_eq!(spans.len(), 3);
        assert_eq!(&text[spans[0].start..spans[0].end], b"one\n");
        assert_eq!(&text[spans[1].start..spans[1].end], b"two\n");
        assert_eq!(&text[spans[2].start..spans[2].end], b"three\n");
    }

    #[test]
    fn build_generates_offsets() {
        let text = b"alpha\n%\nbeta\n";
        let (dat, stats) = build_dat_from_text(text, BuildOptions::default()).expect("build dat");
        assert_eq!(stats.record_count, 2);
        assert_eq!(dat.offsets, vec![0, 8]);
    }
}
