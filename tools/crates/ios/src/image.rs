//! Media: disk images that are read from a file and written to memory only,
//! and the boot tape.  An image can be saved as a new file with what was
//! written to it.
//!
//! The formats are those of the cray-sim project's ready-to-run system
//! (`docs/spec/ios-devices-spec.md`, parts 8.6, 10.2 and 10.3): they
//! are what the surviving COS 1.17 system exists in.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// A disk image: blocks of a fixed size, no header.  Parcels are stored high
/// byte first.  Blocks that are written are kept in memory and the file is
/// never changed; a block that the file does not have reads as zeros.
pub struct Image {
    file: Option<File>,
    path: Option<PathBuf>,
    block: usize,
    written: HashMap<u64, Box<[u8]>>,
}

impl Image {
    /// An image with no file behind it: all zeros.
    pub fn empty(block: usize) -> Image {
        Image {
            file: None,
            path: None,
            block,
            written: HashMap::new(),
        }
    }

    /// The image in the file at `path`.
    pub fn open(path: &Path, block: usize) -> std::io::Result<Image> {
        Ok(Image {
            file: Some(File::open(path)?),
            path: Some(path.to_path_buf()),
            block,
            written: HashMap::new(),
        })
    }

    /// Bytes in a block.
    pub fn block(&self) -> usize {
        self.block
    }

    /// Blocks written since the image was opened.
    pub fn written_blocks(&self) -> usize {
        self.written.len()
    }

    /// Write the image as it now is to a new file at `to`: the file it was
    /// opened from, with the blocks that were written since.  An image with
    /// no file behind it ends with its last written block.
    pub fn save(&self, to: &Path) -> std::io::Result<()> {
        match &self.path {
            Some(from) => {
                std::fs::copy(from, to)?;
            }
            None => {
                File::create(to)?;
            }
        }
        let mut file = std::fs::OpenOptions::new().write(true).open(to)?;
        let mut blocks: Vec<_> = self.written.iter().collect();
        blocks.sort_by_key(|(index, _)| **index);
        for (index, bytes) in blocks {
            file.seek(SeekFrom::Start(index * self.block as u64))?;
            file.write_all(bytes)?;
        }
        file.flush()
    }

    /// Block `index` as parcels.
    pub fn read(&mut self, index: u64) -> Vec<u16> {
        let mut bytes = vec![0u8; self.block];
        if let Some(block) = self.written.get(&index) {
            bytes.copy_from_slice(block);
        } else if let Some(file) = &mut self.file {
            // a short read leaves zeros: the end of a file that is too small
            if file
                .seek(SeekFrom::Start(index * self.block as u64))
                .is_ok()
            {
                let mut got = 0;
                while got < bytes.len() {
                    match file.read(&mut bytes[got..]) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => got += n,
                    }
                }
            }
        }
        bytes
            .chunks(2)
            .map(|b| (b[0] as u16) << 8 | b[1] as u16)
            .collect()
    }

    /// Replace block `index`.  Parcels that `parcels` does not have are zero.
    pub fn write(&mut self, index: u64, parcels: &[u16]) {
        let mut bytes = vec![0u8; self.block].into_boxed_slice();
        for (n, parcel) in parcels.iter().take(self.block / 2).enumerate() {
            bytes[2 * n] = (parcel >> 8) as u8;
            bytes[2 * n + 1] = *parcel as u8;
        }
        self.written.insert(index, bytes);
    }
}

/// What the tape drive found at the end of its last motion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TapeState {
    /// At the load point.
    Beginning,
    /// Behind a record.
    Record,
    /// Behind a file mark.
    FileMark,
    /// Nothing more on the tape.
    End,
}

/// A tape: records and file marks.
///
/// The `.tap` format is a sequence of records, each a 4-byte little-endian
/// length, the data, and the same length again; a length of 0 alone is a
/// file mark.
///
/// A tape file that is mounted on the drive has a fixed length, of which
/// the whole blocks of 512 bytes are the tape.  What is on it ends at the
/// first length whose fourth byte is not zero, or that leads past the end;
/// the drive writes four bytes of all ones behind what it writes.
pub struct Tape {
    /// `None` is a file mark.
    items: Vec<Option<Vec<u8>>>,
    position: usize,
    state: TapeState,
    /// It has no write ring.
    locked: bool,
    /// The bytes its file has room for.
    room: Option<usize>,
}

/// The bytes a record or a file mark takes in a file.
fn file_bytes(item: &Option<Vec<u8>>) -> usize {
    item.as_ref().map_or(4, |data| data.len() + 8)
}

impl Tape {
    /// A tape with nothing on it.
    pub fn blank() -> Tape {
        Tape {
            items: Vec::new(),
            position: 0,
            state: TapeState::Beginning,
            locked: false,
            room: None,
        }
    }

    /// The tape in a file of fixed length, as the drive reads a tape that
    /// is mounted on it.
    pub fn from_file(bytes: &[u8]) -> Tape {
        let room = bytes.len() / 512 * 512;
        let mut items = Vec::new();
        let mut at = 0;
        while at + 4 <= room {
            let n = u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
            let n = n as usize;
            if n >> 24 != 0 {
                break;
            }
            if n == 0 {
                items.push(None);
                at += 4;
                continue;
            }
            if at + n + 8 > room {
                break;
            }
            items.push(Some(bytes[at + 4..at + 4 + n].to_vec()));
            at += n + 8;
        }
        Tape {
            items,
            room: Some(room),
            ..Tape::blank()
        }
    }

    /// The tape as a file: its records and file marks, and behind them
    /// the four bytes that end it if there is room for them.
    pub fn to_file(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for item in &self.items {
            let n = item.as_ref().map_or(0, |data| data.len()) as u32;
            out.extend(n.to_le_bytes());
            if let Some(data) = item {
                out.extend(data);
                out.extend(n.to_le_bytes());
            }
        }
        if self.room.is_none_or(|room| out.len() + 4 <= room) {
            out.extend([0xFF; 4]);
        }
        out
    }

    /// The same tape without a write ring.
    pub fn without_ring(mut self) -> Tape {
        self.locked = true;
        self
    }

    /// Whether it has no write ring.
    pub fn locked(&self) -> bool {
        self.locked
    }

    /// Whether `bytes` more fit behind the heads.
    fn fits(&self, bytes: usize) -> bool {
        let at: usize = self.items[..self.position].iter().map(file_bytes).sum();
        self.room.is_none_or(|room| at + bytes <= room)
    }

    /// The tape in a `.tap` file's bytes.
    pub fn from_tap(bytes: &[u8]) -> Result<Tape, String> {
        let mut items = Vec::new();
        let mut at = 0;
        let length = |at: usize| -> Option<usize> {
            let b = bytes.get(at..at + 4)?;
            Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
        };
        while at < bytes.len() {
            let n = length(at).ok_or("tape image ends inside a record length")?;
            at += 4;
            if n == 0 {
                items.push(None);
                continue;
            }
            let data = bytes
                .get(at..at + n)
                .ok_or("tape image ends inside a record")?;
            at += n;
            if length(at) != Some(n) {
                return Err(format!(
                    "tape record ending at byte {} has no matching length",
                    at
                ));
            }
            at += 4;
            items.push(Some(data.to_vec()));
        }
        Ok(Tape {
            items,
            ..Tape::blank()
        })
    }

    /// Change one byte of a file, counted from the start of the file's
    /// data.  For the patches the cray-sim configuration applies to the
    /// overlay file.  False if the file has no such byte.
    pub fn poke(&mut self, file: usize, mut offset: usize, value: u8) -> bool {
        let mut at = 0;
        for item in self.items.iter_mut() {
            match item {
                None => at += 1,
                Some(_) if at < file => {}
                Some(_) if at > file => return false,
                Some(data) => {
                    if offset < data.len() {
                        data[offset] = value;
                        return true;
                    }
                    offset -= data.len();
                }
            }
        }
        false
    }

    pub fn state(&self) -> TapeState {
        self.state
    }

    pub fn rewind(&mut self) {
        self.position = 0;
        self.state = TapeState::Beginning;
    }

    /// Read the next record: at most `parcels` of it, the rest is passed
    /// over.  At a file mark or the end of the tape nothing is read.
    pub fn read(&mut self, parcels: usize) -> Vec<u16> {
        match self.items.get(self.position) {
            None => {
                self.state = TapeState::End;
                Vec::new()
            }
            Some(None) => {
                self.position += 1;
                self.state = TapeState::FileMark;
                Vec::new()
            }
            Some(Some(data)) => {
                self.position += 1;
                self.state = TapeState::Record;
                data.chunks(2)
                    .take(parcels)
                    .map(|b| (b[0] as u16) << 8 | *b.get(1).unwrap_or(&0) as u16)
                    .collect()
            }
        }
    }

    /// Pass over the next record.  False at a file mark, which is passed
    /// over too, and at the end of the tape.
    pub fn space_forward(&mut self) -> bool {
        match self.items.get(self.position) {
            None => {
                self.state = TapeState::End;
                false
            }
            Some(item) => {
                self.position += 1;
                self.state = if item.is_some() {
                    TapeState::Record
                } else {
                    TapeState::FileMark
                };
                item.is_some()
            }
        }
    }

    /// Back over the record before the heads.  False at a file mark, which
    /// is passed over too, and at the load point.
    pub fn space_backward(&mut self) -> bool {
        if self.position == 0 {
            self.state = TapeState::Beginning;
            return false;
        }
        self.position -= 1;
        let record = self.items[self.position].is_some();
        self.state = if self.position == 0 {
            TapeState::Beginning
        } else if record {
            TapeState::Record
        } else {
            TapeState::FileMark
        };
        record
    }

    /// Write a record at the heads; what was on the tape from there on is
    /// gone.  False if the tape has no room for it: it is at its end then.
    pub fn write(&mut self, parcels: &[u16]) -> bool {
        if !self.fits(2 * parcels.len() + 8) {
            self.state = TapeState::End;
            return false;
        }
        self.items.truncate(self.position);
        self.items
            .push(Some(parcels.iter().flat_map(|p| p.to_be_bytes()).collect()));
        self.position += 1;
        self.state = TapeState::Record;
        true
    }

    /// Write a file mark at the heads.  False if the tape has no room.
    pub fn write_mark(&mut self) -> bool {
        if !self.fits(4) {
            self.state = TapeState::End;
            return false;
        }
        self.items.truncate(self.position);
        self.items.push(None);
        self.position += 1;
        self.state = TapeState::FileMark;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tap(items: &[Option<&[u8]>]) -> Vec<u8> {
        let mut out = Vec::new();
        for item in items {
            match item {
                None => out.extend(0u32.to_le_bytes()),
                Some(data) => {
                    out.extend((data.len() as u32).to_le_bytes());
                    out.extend(*data);
                    out.extend((data.len() as u32).to_le_bytes());
                }
            }
        }
        out
    }

    #[test]
    fn tape_records_and_marks() {
        let bytes = tap(&[Some(&[1, 2, 3, 4]), Some(&[5, 6, 7]), None, Some(&[8, 9])]);
        let mut tape = Tape::from_tap(&bytes).unwrap();
        assert_eq!(tape.state(), TapeState::Beginning);
        assert_eq!(tape.read(10), [0x0102, 0x0304]);
        assert_eq!(tape.state(), TapeState::Record);
        // an odd byte is the high half of a last parcel; a short count cuts
        assert_eq!(tape.read(1), [0x0506]);
        assert_eq!(tape.read(10), []);
        assert_eq!(tape.state(), TapeState::FileMark);
        assert_eq!(tape.read(10), [0x0809]);
        assert_eq!(tape.read(10), []);
        assert_eq!(tape.state(), TapeState::End);
        assert!(tape.space_backward());
        assert!(!tape.space_backward());
        assert_eq!(tape.state(), TapeState::FileMark);
        tape.rewind();
        assert!(tape.space_forward());
        assert_eq!(tape.read(10), [0x0506, 0x0700]);
        // a patch to file 1 and one to file 0, across its records
        assert!(tape.poke(1, 1, 0xAA));
        assert!(tape.poke(0, 5, 0xBB));
        assert!(!tape.poke(0, 7, 0));
        assert!(!tape.poke(2, 0, 0));
        tape.rewind();
        tape.space_forward();
        assert_eq!(tape.read(10), [0x05BB, 0x0700]);
        tape.read(10);
        assert_eq!(tape.read(10), [0x08AA]);
        assert!(Tape::from_tap(&bytes[..9]).is_err());
    }

    #[test]
    fn tape_writes_cut_the_rest() {
        let mut tape = Tape::from_tap(&tap(&[Some(&[1, 2]), Some(&[3, 4])])).unwrap();
        tape.read(1);
        tape.write(&[0xABCD]);
        tape.write_mark();
        assert_eq!(tape.read(1), []);
        assert_eq!(tape.state(), TapeState::End);
        tape.rewind();
        tape.read(1);
        assert_eq!(tape.read(1), [0xABCD]);
        assert!(!tape.space_forward());
    }

    #[test]
    fn tape_files() {
        // a file of two blocks: a record, a file mark, the end, and what does not count
        let mut bytes = tap(&[Some(&[1, 2, 3, 4]), None]);
        bytes.extend([0xFF; 4]);
        bytes.extend([7, 0, 0, 0, 9, 9]);
        bytes.resize(1024 + 100, 0x55);
        let mut tape = Tape::from_file(&bytes);
        assert!(!tape.locked());
        assert_eq!(tape.read(10), [0x0102, 0x0304]);
        assert_eq!(tape.read(10), []);
        assert_eq!(tape.state(), TapeState::FileMark);
        assert_eq!(tape.read(10), []);
        assert_eq!(tape.state(), TapeState::End);
        // what is written ends the tape behind it
        assert!(tape.write(&[0xABCD]));
        let file = tape.to_file();
        assert_eq!(
            file[16..30],
            [2, 0, 0, 0, 0xAB, 0xCD, 2, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF]
        );
        // 1,024 bytes are the tape: 26 are used, and a record of 990 with its lengths is the last that fits
        assert!(!tape.write(&[0; 496]));
        assert_eq!(tape.state(), TapeState::End);
        assert!(tape.write(&[0; 495]));
        assert!(!tape.write_mark());
        assert_eq!(tape.to_file().len(), 1024);
        assert!(Tape::from_file(&tape.to_file()).space_forward());
        assert!(Tape::blank().without_ring().locked());
    }

    #[test]
    fn image_blocks() {
        let mut image = Image::empty(8);
        assert_eq!(image.read(3), [0; 4]);
        image.write(3, &[0x1234, 0x5678]);
        assert_eq!(image.read(3), [0x1234, 0x5678, 0, 0]);
        assert_eq!(image.read(2), [0; 4]);
        assert_eq!(image.written_blocks(), 1);
    }

    #[test]
    fn image_saved_with_what_was_written() {
        let dir = std::env::temp_dir().join(format!("cray-xmp-image-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (from, to) = (dir.join("from.img"), dir.join("to.img"));
        std::fs::write(&from, [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]).unwrap();
        let mut image = Image::open(&from, 4).unwrap();
        image.write(1, &[0xAABB, 0xCCDD]);
        image.write(3, &[0x1122]);
        image.save(&to).unwrap();
        assert_eq!(
            std::fs::read(&to).unwrap(),
            [1, 2, 3, 4, 0xAA, 0xBB, 0xCC, 0xDD, 9, 10, 11, 12, 0x11, 0x22, 0, 0]
        );
        // the file it was opened from is as it was
        assert_eq!(std::fs::read(&from).unwrap().len(), 12);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
