//! Deterministic no-screen NEX V1.2 serialization, independent of the assembler.
//!
//! Layout and original-writer provenance:
//! <https://github.com/stevehill1981/198x/blob/2f50de4ff77f136a1edf65f8412fe35377553bf4/reference/by-system/sinclair-zx-spectrum/sjasmplus-nex-v1.2.md>.
//! Native SjASMPlus 1.21.0 differential images cover the complete byte stream.

use std::fmt;

#[derive(Debug, Clone, Copy)]
pub(crate) enum Error {
    Bank,
    Order,
    Complete,
    Geometry,
    ExtendedRam,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Bank => "[SAVENEX] bank must be in 0..111",
            Self::Order => "[SAVENEX] bank range is behind the save cursor or reversed",
            Self::Complete => "[SAVENEX] all banks are already stored",
            Self::Geometry => "[SAVENEX] Next memory must contain 224 pages of 8192 bytes",
            Self::ExtendedRam => "[SAVENEX] 2MB bank (48..111) stored without 2MbRamReq set in CFG",
        })
    }
}

impl std::error::Error for Error {}

fn bank(index: usize) -> usize {
    match index {
        0 => 5,
        1 => 2,
        2 => 0,
        3 => 1,
        4 => 3,
        5 => 4,
        n => n,
    }
}

fn index(bank: usize) -> Result<usize, Error> {
    match bank {
        0 => Ok(2),
        1 => Ok(3),
        2 => Ok(1),
        3 => Ok(4),
        4 => Ok(5),
        5 => Ok(0),
        6..112 => Ok(bank),
        _ => Err(Error::Bank),
    }
}

pub(crate) struct Image {
    header: [u8; 512],
    data: Vec<u8>,
    last: Option<usize>,
}

impl Image {
    pub(crate) fn new(pc: u16, sp: u16, entry: usize) -> Result<Self, Error> {
        index(entry)?;
        let mut header = [0; 512];
        header[..8].copy_from_slice(b"NextV1.2");
        header[12..14].copy_from_slice(&sp.to_le_bytes());
        header[14..16].copy_from_slice(&pc.to_le_bytes());
        header[135..138].copy_from_slice(&[2, 0, 28]);
        header[139] = entry as u8;
        Ok(Self {
            header,
            data: Vec::new(),
            last: None,
        })
    }

    pub(crate) fn core(&mut self, version: [u8; 3]) {
        self.header[135..138].copy_from_slice(&version);
    }

    pub(crate) fn auto(
        &mut self,
        from: Option<usize>,
        to: Option<usize>,
        pages: &[Vec<u8>],
    ) -> Result<(), Error> {
        if pages.len() != 224 || pages.iter().any(|p| p.len() != 8192) {
            return Err(Error::Geometry);
        }
        if self.last == Some(111) {
            return Err(Error::Complete);
        }
        let start = from
            .map(index)
            .transpose()?
            .unwrap_or(self.last.map_or(0, |n| n + 1));
        let end = to.map(index).transpose()?.unwrap_or(111);
        if end < start || self.last.is_some_and(|last| start <= last) {
            return Err(Error::Order);
        }
        for i in start..=end {
            self.last = Some(i);
            let b = bank(i);
            let halves = &pages[b * 2..b * 2 + 2];
            if halves.iter().all(|p| p.iter().all(|v| *v == 0)) {
                continue;
            }
            self.header[18 + b] = 1;
            self.header[9] += 1;
            for half in halves {
                self.data.extend_from_slice(half);
            }
        }
        Ok(())
    }

    pub(crate) fn finish(self) -> Result<Vec<u8>, Error> {
        if self.header[66..130].iter().any(|v| *v != 0) {
            return Err(Error::ExtendedRam);
        }
        let mut out = Vec::with_capacity(512 + self.data.len());
        out.extend_from_slice(&self.header);
        out.extend_from_slice(&self.data);
        Ok(out)
    }
}
