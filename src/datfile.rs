use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use tracing::{debug, instrument};

pub const STRFILE_VERSION: u32 = 2;
pub const STR_RANDOM: u32 = 0x1;
pub const STR_ORDERED: u32 = 0x2;
pub const STR_ROTATED: u32 = 0x4;
pub const HEADER_BYTES: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DatHeader {
    pub version: u32,
    pub numstr: u32,
    pub longlen: u32,
    pub shortlen: u32,
    pub flags: u32,
    pub delim: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatFile {
    pub header: DatHeader,
    pub offsets: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct FortuneFile {
    pub text_path: PathBuf,
    pub dat_path: PathBuf,
    pub dat: DatFile,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LengthFilter {
    Any,
    Short { threshold: usize },
    Long { threshold: usize },
}

impl LengthFilter {
    pub fn accepts(self, len: usize) -> bool {
        match self {
            Self::Any => true,
            Self::Short { threshold } => len <= threshold,
            Self::Long { threshold } => len > threshold,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DatValidationError(&'static str);

impl Display for DatValidationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for DatValidationError {}

impl DatFile {
    #[instrument(skip_all, fields(path = %path.display()))]
    pub fn read_from_path(path: &Path) -> Result<Self> {
        let bytes = fs::read(path)
            .with_context(|| format!("failed reading dat file {}", path.display()))?;
        Self::read_from_bytes(&bytes)
    }

    #[instrument(skip_all)]
    pub fn read_from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < HEADER_BYTES {
            bail!(DatValidationError("dat file shorter than header"));
        }

        let version = be_u32(&bytes[0..4]);
        let numstr = be_u32(&bytes[4..8]);
        let longlen = be_u32(&bytes[8..12]);
        let shortlen = be_u32(&bytes[12..16]);
        let flags = be_u32(&bytes[16..20]);
        let delim = bytes[20];

        let expected_offsets_bytes = (numstr as usize)
            .checked_mul(4)
            .ok_or_else(|| DatValidationError("offset table size overflow"))?;
        let expected_total = HEADER_BYTES
            .checked_add(expected_offsets_bytes)
            .ok_or_else(|| DatValidationError("dat size overflow"))?;
        if bytes.len() < expected_total {
            bail!(DatValidationError("dat file missing offset entries"));
        }

        let mut offsets = Vec::with_capacity(numstr as usize);
        for raw in bytes[HEADER_BYTES..expected_total].chunks_exact(4) {
            offsets.push(be_u32(raw));
        }

        let header = DatHeader {
            version,
            numstr,
            longlen,
            shortlen,
            flags,
            delim,
        };

        debug!(?header, offsets = offsets.len(), "parsed dat file");
        Ok(Self { header, offsets })
    }

    #[instrument(skip_all, fields(path = %path.display()))]
    pub fn write_to_path(&self, path: &Path) -> Result<()> {
        let encoded = self.to_bytes()?;
        fs::write(path, encoded).with_context(|| format!("failed writing {}", path.display()))
    }

    #[instrument(skip_all)]
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        if self.offsets.len() > (u32::MAX as usize) {
            bail!(DatValidationError("too many offsets for STRFILE u32"));
        }
        if self.header.numstr != self.offsets.len() as u32 {
            bail!(DatValidationError(
                "header numstr does not match offset count"
            ));
        }

        let mut out = Vec::with_capacity(HEADER_BYTES + (self.offsets.len() * 4));
        out.extend_from_slice(&self.header.version.to_be_bytes());
        out.extend_from_slice(&self.header.numstr.to_be_bytes());
        out.extend_from_slice(&self.header.longlen.to_be_bytes());
        out.extend_from_slice(&self.header.shortlen.to_be_bytes());
        out.extend_from_slice(&self.header.flags.to_be_bytes());
        out.push(self.header.delim);
        out.extend_from_slice(&[0_u8; 3]);
        for offset in &self.offsets {
            out.extend_from_slice(&offset.to_be_bytes());
        }
        Ok(out)
    }
}

impl FortuneFile {
    #[instrument(skip_all, fields(path = %text_path.display()))]
    pub fn open(text_path: &Path) -> Result<Self> {
        let dat_path = dat_path_for_text(text_path);
        let dat = DatFile::read_from_path(&dat_path)?;
        let bytes = fs::read(text_path)
            .with_context(|| format!("failed reading fortune text {}", text_path.display()))?;
        let db = Self {
            text_path: text_path.to_path_buf(),
            dat_path,
            dat,
            bytes,
        };
        db.validate_offsets()?;
        debug!(
            file = %db.text_path.display(),
            fortunes = db.dat.offsets.len(),
            "opened fortune file"
        );
        Ok(db)
    }

    #[instrument(skip_all, fields(path = %self.text_path.display()))]
    pub fn validate_offsets(&self) -> Result<()> {
        let file_len = self.bytes.len() as u64;
        for offset in &self.dat.offsets {
            if (*offset as u64) > file_len {
                bail!(
                    "offset {} is out of range for file {} bytes in {}",
                    offset,
                    file_len,
                    self.text_path.display()
                );
            }
        }
        Ok(())
    }

    pub fn record_count(&self) -> usize {
        self.dat.offsets.len()
    }

    pub fn span(&self, index: usize) -> Result<RecordSpan> {
        let start = *self
            .dat
            .offsets
            .get(index)
            .with_context(|| format!("record index {index} out of range"))?
            as usize;
        let end = self.find_delimiter_start(start).unwrap_or(self.bytes.len());
        if start > end || end > self.bytes.len() {
            bail!(
                "invalid record span [{start}, {end}) for file size {}",
                self.bytes.len()
            );
        }
        Ok(RecordSpan { start, end })
    }

    pub fn record_bytes(&self, index: usize) -> Result<&[u8]> {
        let span = self.span(index)?;
        let end = self.delimiter_trimmed_end(span.start, span.end);
        Ok(&self.bytes[span.start..end])
    }

    pub fn record_text_lossy(&self, index: usize) -> Result<String> {
        Ok(String::from_utf8_lossy(self.record_bytes(index)?).into_owned())
    }

    pub fn candidate_indices(&self, filter: LengthFilter) -> Result<Vec<usize>> {
        let mut out = Vec::new();
        for idx in 0..self.record_count() {
            let len = self.record_bytes(idx)?.len();
            if filter.accepts(len) {
                out.push(idx);
            }
        }
        Ok(out)
    }

    fn delimiter_trimmed_end(&self, start: usize, bound_end: usize) -> usize {
        let mut cursor = start;
        while cursor < bound_end {
            let line_end = self.bytes[cursor..bound_end]
                .iter()
                .position(|b| *b == b'\n')
                .map(|rel| cursor + rel)
                .unwrap_or(bound_end);
            let mut content_end = line_end;
            if content_end > cursor && self.bytes[content_end - 1] == b'\r' {
                content_end -= 1;
            }
            let is_delim_line =
                content_end == cursor + 1 && self.bytes[cursor] == self.dat.header.delim;
            if is_delim_line {
                return cursor;
            }
            cursor = if line_end < bound_end {
                line_end + 1
            } else {
                line_end
            };
        }
        bound_end
    }

    fn find_delimiter_start(&self, start: usize) -> Option<usize> {
        let mut cursor = start;
        while cursor < self.bytes.len() {
            let line_end = self.bytes[cursor..]
                .iter()
                .position(|b| *b == b'\n')
                .map(|rel| cursor + rel)
                .unwrap_or(self.bytes.len());
            let mut content_end = line_end;
            if content_end > cursor && self.bytes[content_end - 1] == b'\r' {
                content_end -= 1;
            }
            let is_delim_line =
                content_end == cursor + 1 && self.bytes[cursor] == self.dat.header.delim;
            if is_delim_line {
                return Some(cursor);
            }
            cursor = if line_end < self.bytes.len() {
                line_end + 1
            } else {
                line_end
            };
        }
        None
    }
}

pub fn dat_path_for_text(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.dat", path.display()))
}

fn be_u32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dat_round_trip() {
        let dat = DatFile {
            header: DatHeader {
                version: STRFILE_VERSION,
                numstr: 3,
                longlen: 20,
                shortlen: 4,
                flags: STR_ORDERED,
                delim: b'%',
            },
            offsets: vec![0, 12, 40],
        };

        let bytes = dat.to_bytes().expect("encode");
        let decoded = DatFile::read_from_bytes(&bytes).expect("decode");
        assert_eq!(decoded, dat);
    }
}
