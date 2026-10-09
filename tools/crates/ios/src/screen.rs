//! An Ampex Dialogue 80, the terminal the IOS software drives its consoles as:
//! 24 lines of 80 characters and a status line, two or four pages.
//!
//! The model does what the terminal's firmware does, routine by routine and
//! with the firmware's own variables, so that what the firmware does by
//! accident comes out by the same accident.  It was made from notes taken of
//! the firmware and is checked against the firmware itself, run on a model of
//! the terminal's hardware (`tools/py/d80test.py`).  `[D80 n]` is section n of
//! those notes; a four-digit hexadecimal number is an address in the program
//! ROM, given where the terminal does something one would not expect.
//!
//! What is modelled is what a host and the person at the keyboard can see:
//! the cells of every page, the cursor, the modes, the status line, the bytes
//! the terminal sends, the bell.  Every byte received and every key is worked
//! off to the end before the next one: the firmware's main loop is turned
//! until a turn changes nothing.  The real terminal takes bytes into a queue
//! from an interrupt and works them off later, so a host that sends faster
//! than the terminal works can see a few things happen in another order
//! (`ESC J` is one, [D80 4.4]).
//!
//! The terminal's memory [D80 3.1]: a cell holds a 7-bit character, a protect
//! bit (shown as half intensity) and four attributes (reverse, blank, flash,
//! underline).  The firmware writes cells through a latch that holds the
//! protect bit and the attributes to be written and three enables: whether a
//! write stores the character, the protect bit, the attributes.  Normally the
//! character and the protect bit are written and the attributes are left as
//! they are.
//!
//! Nothing is moved to scroll [D80 2.2].  Each page has `bottom`, the row of
//! its memory that is shown as the last line; line n of the screen is memory
//! row (bottom + 1 + n) mod 24.
//!
//! No printer is connected.  The firmware then never finishes a print
//! [D80 4.4]: after `ESC P` the status line shows PTG for good, and after
//! `ESC J` the terminal takes at most a few more bytes and then leaves
//! everything it receives in its queue, until that is full.  The host line
//! is taken to be free at all times: what the terminal sends, it has sent.
//!
//! Two things the firmware can do have no state to model.  It can go into a
//! loop for good ([`Screen::hung`]), and with function keys it can lose
//! track of its own memory ([`Screen::lost`]).
//!
//! Left out: the look of the screen, auto-repeat and the scanning of the
//! keyboard, the baud rates, the length of the bell, the break that the
//! BREAK key sends, and key codes that no key of the keyboard has.

use std::collections::VecDeque;

pub const LINES: usize = 24;
pub const COLUMNS: usize = 80;

/// The modes, in the order of their names on the status line [D80 2.7]:
/// auto flip, write attribute, write protect, block, programmable key (one is
/// being programmed or played), half duplex, protect, program.
const FLP: u8 = 0x01;
const ATB: u8 = 0x02;
const WPT: u8 = 0x04;
const BLK: u8 = 0x08;
const PGK: u8 = 0x10;
const HDX: u8 = 0x20;
const PRT: u8 = 0x40;
const PGM: u8 = 0x80;

/// The attribute of a cell that makes it flash.
const FLASH: u8 = 4;

/// One position of the screen.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Cell {
    /// The character, 7 bits.  00 is what a cleared screen holds; 01-0B are
    /// the line-drawing characters; 0C-1F the pictures of control codes.
    pub code: u8,
    /// Written in write protect mode; a field of such cells is skipped by
    /// the cursor in protect mode.
    pub protect: bool,
    pub attributes: u8,
}

/// What the terminal reads from its switches and jumpers [D80 1.3, 1.6].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Switches {
    /// Typed characters are shown as well as sent.
    pub half_duplex: bool,
    /// The column the cursor rings the bell on when it moves right into it;
    /// 0 for no margin bell.
    pub bell_column: u8,
    /// The cursor goes to the next line behind column 79.
    pub wrap: bool,
    /// Has no part in what is modelled.
    pub fifty_hertz: bool,
    /// 2 or 4.
    pub pages: usize,
    /// The key lock: the keyboard is dead but for CTRL+CLEAR.
    pub locked: bool,
    /// The twenty programmable keys [D80 5.5].  Without them `ESC e` and its
    /// two parameters are taken and nothing more happens.
    pub function_keys: bool,
}

impl Default for Switches {
    fn default() -> Switches {
        Switches {
            half_duplex: false,
            bell_column: 0,
            wrap: true,
            fifty_hertz: false,
            pages: 2,
            locked: false,
            function_keys: false,
        }
    }
}

type Page = [[Cell; COLUMNS]; LINES];

/// Where a page is in the firmware's address space: 8000h + row * 100h +
/// column.  The firmware keeps places on the screen as such addresses.
const PAGE_RAM: u16 = 0x8000;

/// The memory the firmware keeps the function keys in: a table of twenty
/// starts and ends, then the keys' bytes [D80 5.5].
const KEY_TABLE: u16 = 0x4295;
const KEY_STORE: u16 = 0x42E5;
const KEY_MEMORY_END: u16 = 0x4400;

/// The texts of the status line [D80 6].
const MODE_NAMES: &[u8; 32] = b"FLP ATB WPT BLK PGK HDX PRT PGM ";
const MESSAGES: [&[u8; 8]; 9] = [
    b"        ",
    b"MODE ERR",
    b"PROG ERR",
    b"CURS ERR",
    b"PROT ERR",
    b"BAD ROM ",
    b"BAD RAM ",
    b"BAD UART",
    b"FUNC ERR",
];

/// The numbers of the messages.
const MODE_ERR: u8 = 2;
const PROG_ERR: u8 = 3;
const CURS_ERR: u8 = 4;
const PROT_ERR: u8 = 5;

/// The letters that switch reverse, blank, flash and underline on and off.
const ATTRIBUTE_LETTERS: &[u8; 8] = b"jkpqnolm";

/// The codes the keys send [D80 5.2]: thirteen rows of eight keys, plain,
/// with CTRL, with SHIFT, and with caps lock on.  00-7F are characters;
/// 80 is no key; 81 caps lock, 82 reset, 83 break, 8F the test pattern;
/// 84-97 and A0-FF are functions the terminal carries out by itself.
const KEYS_PLAIN: [u8; 104] = [
    0x5C, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x1B, 0x71, 0x77, 0x65, 0x72, 0x74, 0x79, 0x75,
    0x81, 0x80, 0x61, 0x73, 0x64, 0x66, 0x67, 0x68, 0x83, 0x80, 0x7A, 0x78, 0x63, 0x76, 0x62, 0x6E,
    0x6D, 0x2C, 0x80, 0x2E, 0x2F, 0x80, 0x20, 0x80, 0x6A, 0x6B, 0x6C, 0x3B, 0x5B, 0x5D, 0x1F, 0x94,
    0x69, 0x6F, 0x70, 0x5E, 0x0A, 0x0D, 0xD4, 0xD9, 0x38, 0x39, 0x30, 0x3A, 0x2D, 0x40, 0x09, 0x90,
    0x0D, 0x2E, 0x30, 0x2C, 0x97, 0x0A, 0x96, 0x80, 0x0D, 0x33, 0x32, 0x31, 0x0C, 0x1E, 0x7F, 0x80,
    0x09, 0x36, 0x35, 0x34, 0x95, 0x0B, 0x08, 0x80, 0x2D, 0x39, 0x38, 0x37, 0x8B, 0xD0, 0xB4, 0x80,
    0x80, 0x87, 0x85, 0x89, 0x92, 0x91, 0xB5, 0x80,
];
const KEYS_CTRL: [u8; 104] = [
    0x1C, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x1B, 0x11, 0x17, 0x05, 0x12, 0x14, 0x19, 0x15,
    0x81, 0x80, 0x01, 0x13, 0x04, 0x06, 0x07, 0x08, 0x83, 0x80, 0x1A, 0x18, 0x03, 0x16, 0x02, 0x0E,
    0x0D, 0x80, 0x80, 0x80, 0x80, 0x80, 0x20, 0x80, 0x0A, 0x0B, 0x0C, 0x80, 0x1B, 0x1D, 0x80, 0x80,
    0x09, 0x0F, 0x10, 0x1E, 0x0A, 0x0D, 0x80, 0x80, 0x80, 0x80, 0x1F, 0x80, 0x80, 0x00, 0x09, 0x80,
    0x0D, 0x80, 0x30, 0x2C, 0x80, 0x0A, 0x80, 0x80, 0x0D, 0x33, 0x32, 0x31, 0x0C, 0x1E, 0x80, 0x80,
    0x09, 0x36, 0x35, 0x34, 0x80, 0x0B, 0x08, 0x80, 0x2D, 0x39, 0x38, 0x37, 0x82, 0x80, 0x80, 0x80,
    0x8F, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
];
const KEYS_SHIFT: [u8; 104] = [
    0x7C, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x1B, 0x51, 0x57, 0x45, 0x52, 0x54, 0x59, 0x55,
    0x81, 0x80, 0x41, 0x53, 0x44, 0x46, 0x47, 0x48, 0x83, 0x80, 0x5A, 0x58, 0x43, 0x56, 0x42, 0x4E,
    0x4D, 0x3C, 0x80, 0x3E, 0x3F, 0x80, 0x20, 0x80, 0x4A, 0x4B, 0x4C, 0x2B, 0x7B, 0x7D, 0x8A, 0x94,
    0x49, 0x4F, 0x50, 0x7E, 0x0A, 0x0D, 0xF4, 0xF9, 0x28, 0x29, 0x5F, 0x2A, 0x3D, 0x60, 0x09, 0x84,
    0x0D, 0x3E, 0x30, 0x2C, 0x97, 0x0A, 0x96, 0x80, 0x0D, 0x33, 0x32, 0x31, 0x0C, 0x1E, 0x7F, 0x80,
    0x09, 0x36, 0x35, 0x34, 0x95, 0x0B, 0x08, 0x80, 0x2D, 0x39, 0x38, 0x37, 0x8C, 0xD0, 0xB6, 0x80,
    0x80, 0x86, 0x84, 0x88, 0x93, 0x91, 0xB7, 0x80,
];
const KEYS_CAPS: [u8; 104] = [
    0x5C, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x1B, 0x51, 0x57, 0x45, 0x52, 0x54, 0x59, 0x55,
    0x81, 0x80, 0x41, 0x53, 0x44, 0x46, 0x47, 0x48, 0x83, 0x80, 0x5A, 0x58, 0x43, 0x56, 0x42, 0x4E,
    0x4D, 0x2C, 0x80, 0x2E, 0x2F, 0x80, 0x20, 0x80, 0x4A, 0x4B, 0x4C, 0x3B, 0x5B, 0x5D, 0x1F, 0x94,
    0x49, 0x4F, 0x50, 0x5E, 0x0A, 0x0D, 0xD4, 0xD9, 0x38, 0x39, 0x30, 0x3A, 0x2D, 0x40, 0x09, 0x90,
    0x0D, 0x2E, 0x30, 0x2C, 0x97, 0x0A, 0x96, 0x80, 0x0D, 0x33, 0x32, 0x31, 0x0C, 0x1E, 0x7F, 0x80,
    0x09, 0x36, 0x35, 0x34, 0x95, 0x0B, 0x08, 0x80, 0x2D, 0x39, 0x38, 0x37, 0x8B, 0xD0, 0xB4, 0x80,
    0x80, 0x87, 0x85, 0x89, 0x92, 0x91, 0xB5, 0x80,
];

/// The cell that shows control code `c` (00-1A) in program mode [D80 2.9].
/// Twenty-one control codes have a picture of their own; the other eleven
/// are shown as the line-drawing characters 01-0B.  What is sent of the
/// screen goes back through the same table [D80 4.3].
const CONTROL_PICTURE: [u8; 27] = [
    0x00, 0x0E, 0x0F, 0x10, 0x04, 0x05, 0x12, 0x14, 0x16, 0x17, 0x19, 0x1A, 0x0C, 0x0D, 0x03, 0x01,
    0x02, 0x11, 0x06, 0x13, 0x07, 0x15, 0x08, 0x09, 0x18, 0x0A, 0x0B,
];

/// The escape functions that are carried out in program mode too.
fn works_in_program_mode(letter: u8) -> bool {
    matches!(
        letter,
        b'4'..=b'7' | b'P' | b'T' | b'Y' | b'd' | b't' | b'y'
    )
}

/// The escape function a key of the codes 80h-97h stands for [D80 5.3].
fn function_of_key(code: u8) -> Option<u8> {
    Some(match code {
        0x84 => b'B',
        0x85 => b'C',
        0x86 => b')',
        0x87 => b'(',
        0x88 => b'&',
        0x89 => b'\'',
        0x8A => b'N',
        0x8B => b';',
        0x8C => b':',
        0x90 => b'I',
        0x91 => b'1',
        0x92 => b'2',
        0x93 => b'3',
        0x94 => b'E',
        0x95 => b'R',
        0x96 => b'Q',
        0x97 => b'W',
        _ => return None,
    })
}

fn cell_addr(row: u8, column: u8) -> u16 {
    PAGE_RAM.wrapping_add(u16::from(row) << 8 | u16::from(column))
}

fn low(addr: u16) -> u8 {
    addr as u8
}

fn high(addr: u16) -> u8 {
    (addr >> 8) as u8
}

/// The pair of a pointer and its limit that a step is taken on.
#[derive(Clone, Copy)]
enum Walk {
    Send,
    Print,
    Work,
}

#[derive(Clone)]
pub struct Screen {
    switches: Switches,

    // --- the hardware the firmware writes to
    pages: Vec<Page>,
    status: [Cell; COLUMNS],
    /// The latch cells are written through: bit 7 the protect bit, bits 6-3
    /// underline, flash, blank and reverse, bits 2, 1 and 0 whether a write
    /// stores the character, the protect bit, the attributes [D80 1.7].
    latch: u8,
    /// Bits 1-0 the page shown, bit 3 program mode, bits 5-4 the page the
    /// firmware addresses, bit 7 the lamp of the caps lock key [D80 1.2].
    port0: u8,
    /// A cell is read as its attributes, not as its character.
    readback: bool,

    // --- the firmware's variables
    /// What was received and waits to be shown: a ring of 256.
    dispq: [u8; 256],
    dispq_in: u8,
    dispq_out: u8,
    /// The keys that wait: a ring of 16.
    keyq: [u8; 16],
    keyq_in: u8,
    keyq_out: u8,
    /// The latch as it is between the firmware's own uses of it: the
    /// attributes that are on, write protect, write attribute.
    shadow: u8,
    mode: u8,
    /// Bit 7: write the status line anew; bits 6-4: receive errors; bits 3-0:
    /// the number of a message.  Errors are ORed into it [D80 2.8].
    status_req: u8,
    /// The collector of escape sequences: 0 idle, FFh ESC seen, else the
    /// bytes still to come [D80 2.1].
    esc_cnt: u8,
    esc_len: u8,
    esc_buf: [u8; 6],
    rx_byte: u8,
    cur_byte: u8,
    /// Not zero: the byte just worked off moves the cursor right.
    advance: u8,
    page: u8,
    page_max: u8,
    cur_col: u8,
    /// A row of memory, not a line of the screen.
    cur_row: u8,
    kbd_locked: u8,
    printing: u8,
    transparent: u8,
    /// The cursor is shown on the status line: after CURS ERR and PROT ERR.
    cur_in_status: u8,
    bottom: [u8; 4],
    /// A place on the screen many routines work with, the limit of its walk
    /// and the direction, +1 or -1.  They are left as the last routine had
    /// them, and some routines use them without setting them.
    ptr: u16,
    limit: u16,
    step: u8,
    /// The key lock as the status line last showed it.
    lock_shown: u8,
    caps: u8,
    key: u8,
    /// The queue a key had to go into was full: the key stays.
    q_full: u8,

    send_attr: u8,
    send_prot: u8,
    send_ptr: u16,
    send_end: u16,
    sending: u8,
    send_all: u8,

    prn_prot: u8,
    prn_pgm: u8,
    prn_page: u8,
    prn_ptr: u16,
    prn_end: u16,
    prn_ack: u8,
    prn_done: u8,
    /// Bytes that wait for the printer.  None ever leaves.
    prn_queued: u8,

    wrapped: u8,
    /// A bit for each column, eight columns to a byte.  The firmware has nine
    /// bytes for them and uses ten: the tenth is `skip_prot`.
    tabs: [u8; 9],
    /// Erasing leaves protected cells alone.  Also the tab stops of columns
    /// 72-79 [D80 2.6.5].
    skip_prot: u8,
    fill: u8,
    seek_state: u8,
    seek_want: u8,

    // what insert and delete pass to the routines they share
    ch_step: u16,
    ch_back: u16,
    pg_last: u8,
    line_dir: u8,
    src_row: u8,
    dst_row: u8,
    src_page: u8,
    dst_page: u8,
    cur_lrow: u8,
    delta: u8,

    // --- function keys
    fk_recording: u8,
    fk_to_host: u8,
    fk_index: u8,
    fk_ptr: u16,
    fk_free: u16,
    fk_memory: Vec<u8>,
    /// The firmware would have read or written memory that is not the
    /// function keys': from here on the model does not follow it.
    fk_lost: bool,

    /// The firmware has gone into a loop that it does not leave.
    hung: bool,

    sent: VecDeque<u8>,
    /// Counts what a turn of the main loop changed, to see when it changed
    /// nothing.  (A turn can be busy and change nothing: a function key
    /// played into a full queue writes COMM ERR:O anew at every turn.)
    work: u64,
    /// Times the bell was rung, the margin bell too.
    pub bells: u32,
}

impl Default for Screen {
    fn default() -> Screen {
        Screen::new(Switches::default())
    }
}

/// What a turn of the main loop can change, but the screen itself.
#[derive(PartialEq, Eq)]
struct Progress([u16; 40], u64);

impl Screen {
    /// The modes as bits of [`Screen::mode`].
    pub const FLP: u8 = FLP;
    pub const ATB: u8 = ATB;
    pub const WPT: u8 = WPT;
    pub const BLK: u8 = BLK;
    pub const PGK: u8 = PGK;
    pub const HDX: u8 = HDX;
    pub const PRT: u8 = PRT;
    pub const PGM: u8 = PGM;

    /// The attributes as bits of [`Cell::attributes`] [D80 3.2].
    pub const REVERSE: u8 = 1;
    pub const BLANK: u8 = 2;
    pub const FLASH: u8 = FLASH;
    pub const UNDERLINE: u8 = 8;

    /// A terminal that has been switched on [D80 7.2]: every cell 00, the
    /// cursor home on page 1, no mode but half duplex if the switch says so,
    /// no attributes, no tab stops.
    pub fn new(switches: Switches) -> Screen {
        let pages = if switches.pages == 4 { 4 } else { 2 };
        let switches = Switches {
            pages,
            bell_column: switches.bell_column & 0x7F,
            ..switches
        };
        let blank = Cell {
            code: b' ',
            protect: false,
            attributes: 0,
        };
        let mut screen = Screen {
            pages: vec![[[Cell::default(); COLUMNS]; LINES]; pages],
            status: [blank; COLUMNS],
            latch: 0x06,
            port0: 0,
            readback: false,
            dispq: [0; 256],
            dispq_in: 0,
            dispq_out: 0,
            keyq: [0; 16],
            keyq_in: 0,
            keyq_out: 0,
            shadow: 0x06,
            mode: if switches.half_duplex { HDX } else { 0 },
            status_req: 0x80,
            esc_cnt: 0,
            esc_len: 0,
            esc_buf: [0; 6],
            rx_byte: 0,
            cur_byte: 0,
            advance: 0,
            page: 0,
            page_max: pages as u8 - 1,
            cur_col: 0,
            cur_row: 0,
            kbd_locked: 0,
            printing: 0,
            transparent: 0,
            cur_in_status: 0,
            bottom: [0x17; 4],
            ptr: PAGE_RAM,
            limit: 0,
            step: 0,
            lock_shown: 0,
            caps: 0,
            key: 0,
            q_full: 0,
            send_attr: 0,
            send_prot: 0,
            send_ptr: 0,
            send_end: 0,
            sending: 0,
            send_all: 0,
            prn_prot: 0,
            prn_pgm: 0,
            prn_page: 0,
            prn_ptr: 0,
            prn_end: 0,
            prn_ack: 0,
            prn_done: 0xFF,
            prn_queued: 0,
            wrapped: 0,
            tabs: [0; 9],
            skip_prot: 0,
            fill: 0,
            seek_state: 0,
            seek_want: 0,
            ch_step: 0,
            ch_back: 0,
            pg_last: 0,
            line_dir: 0,
            src_row: 0,
            dst_row: 0,
            src_page: 0,
            dst_page: 0,
            cur_lrow: 0,
            delta: 0,
            fk_recording: 0,
            fk_to_host: 0,
            fk_index: 0,
            fk_ptr: 0x5C00,
            fk_free: KEY_STORE,
            fk_memory: vec![0; usize::from(KEY_MEMORY_END - KEY_TABLE)],
            fk_lost: false,
            hung: false,
            sent: VecDeque::new(),
            work: 0,
            bells: 0,
            switches,
        };
        screen.settle();
        screen
    }

    /// A terminal with the switches as `set` leaves them.
    pub fn with(set: impl FnOnce(&mut Switches)) -> Screen {
        let mut switches = Switches::default();
        set(&mut switches);
        Screen::new(switches)
    }

    /// The screen after `bytes` have been sent to it.
    pub fn of(bytes: &[u8]) -> Screen {
        let mut screen = Screen::default();
        for &b in bytes {
            screen.put(b);
        }
        screen
    }

    /// One character from the line.
    pub fn put(&mut self, byte: u8) {
        if self.hung {
            return;
        }
        self.receive(byte);
        self.settle();
    }

    /// A key pressed and let go, by the code it sends ([`Screen::key_code`]):
    /// 00-7F a character, 81-97 and A0-FF the terminal's own functions
    /// [D80 5.3].  80 is a key that sends nothing (SHIFT, CTRL).  A code that
    /// no key has does nothing here; the firmware never gets one, and would
    /// take some for escape functions whose parameters are not there.
    pub fn key(&mut self, code: u8) {
        self.press(code, None);
    }

    /// A key, by its code as for [`Screen::key`], pressed while PROG A (or
    /// PROG B, if `b`) is held: plays a function key [D80 5.5].  The key's
    /// number is its code less 30h, and ten more for PROG B: the digits are
    /// the keys 0 to 9, and with SHIFT or caps lock the letters A, B and C
    /// are the keys B7, B8 and B9.
    pub fn prog_key(&mut self, b: bool, code: u8) {
        self.press(code, Some(b));
    }

    /// The code of the key at `row` (0-12), `bit` (0-7) of the keyboard's
    /// matrix [D80 5.2].  CTRL goes before SHIFT, SHIFT before caps lock.
    pub fn key_code(row: usize, bit: usize, shift: bool, ctrl: bool, caps: bool) -> u8 {
        Screen::keys(shift, ctrl, caps)[row * 8 + bit]
    }

    /// The table of the 104 codes for a state of SHIFT, CTRL and caps lock.
    pub fn keys(shift: bool, ctrl: bool, caps: bool) -> &'static [u8; 104] {
        if ctrl {
            &KEYS_CTRL
        } else if shift {
            &KEYS_SHIFT
        } else if caps {
            &KEYS_CAPS
        } else {
            &KEYS_PLAIN
        }
    }

    /// The bytes the terminal has sent to the host since the last call.
    pub fn take_sent(&mut self) -> Vec<u8> {
        self.sent.drain(..).collect()
    }

    // ------------------------------------------------------------ what is seen

    /// Line `n` of the page shown as text, without the blanks at its end.
    /// An empty cell (00) is a blank; the line-drawing characters are `+`
    /// for corners, tees and the cross, `-` and `|`; a picture of a control
    /// code is `?`.
    pub fn line(&self, n: usize) -> String {
        let text: String = (0..COLUMNS)
            .map(|column| match self.line_cell(self.page(), n, column).code {
                0 => ' ',
                0x09 => '-',
                0x0A => '|',
                0x01..=0x0B => '+',
                c @ 0x20..=0x7E => c as char,
                _ => '?',
            })
            .collect();
        text.trim_end_matches(' ').to_string()
    }

    /// The lines, without the empty ones at the bottom.
    pub fn text(&self) -> String {
        let mut lines: Vec<String> = (0..LINES).map(|n| self.line(n)).collect();
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        lines.join("\n")
    }

    /// Where the cursor is: line and column, from 0.
    pub fn cursor(&self) -> (usize, usize) {
        (
            usize::from(self.mem_to_row(self.cur_row, self.page)),
            usize::from(self.cur_col),
        )
    }

    /// The cursor as the firmware keeps it: row of memory and column.
    pub fn cursor_memory(&self) -> (usize, usize) {
        (usize::from(self.cur_row), usize::from(self.cur_col))
    }

    /// The cursor is shown in the status line, where CURS ERR and PROT ERR
    /// put it.  Characters still go to the place [`Screen::cursor`] gives.
    pub fn cursor_in_status(&self) -> bool {
        self.cur_in_status & 1 != 0
    }

    /// Pages fitted: 2 or 4.
    pub fn pages(&self) -> usize {
        self.pages.len()
    }

    /// The page shown and written, from 0.
    pub fn page(&self) -> usize {
        usize::from(self.page)
    }

    /// The row of `page`'s memory that is its last line.
    pub fn bottom(&self, page: usize) -> usize {
        usize::from(self.bottom[page])
    }

    /// A cell by its place in memory.
    pub fn cell(&self, page: usize, memory_row: usize, column: usize) -> Cell {
        self.pages[page][memory_row][column]
    }

    /// A cell by its place on the screen.
    pub fn line_cell(&self, page: usize, line: usize, column: usize) -> Cell {
        self.pages[page][(self.bottom(page) + 1 + line) % LINES][column]
    }

    /// The modes: [`Screen::FLP`] and the others.
    pub fn mode(&self) -> u8 {
        self.mode
    }

    /// What a write stores now: bit 7 the protect bit, bits 6-3 underline,
    /// flash, blank, reverse, bit 0 whether the attributes are stored
    /// (write attribute mode).  Bits 2 and 1 are set.
    pub fn latch(&self) -> u8 {
        self.shadow
    }

    /// The attributes that are switched on, as in [`Cell::attributes`].
    pub fn attributes(&self) -> u8 {
        self.shadow >> 3 & 0x0F
    }

    /// Whether `column` has a tab stop.
    pub fn tab_stop(&self, column: usize) -> bool {
        self.tab_bytes()[column / 8] >> (column % 8) & 1 != 0
    }

    /// The tab stops as the firmware keeps them, column 0 in bit 0 of the
    /// first byte.  The last byte is in use for something else as well
    /// [D80 2.6.5].
    pub fn tab_bytes(&self) -> [u8; 10] {
        let mut bytes = [self.skip_prot; 10];
        bytes[..9].copy_from_slice(&self.tabs);
        bytes
    }

    /// The keyboard is locked, by `ESC #` or by the key lock.
    pub fn keyboard_locked(&self) -> bool {
        self.kbd_locked & 1 != 0 || self.switches.locked
    }

    pub fn caps_lock(&self) -> bool {
        self.caps & 1 != 0
    }

    /// A print has been started.  Without a printer none ends.
    pub fn printing(&self) -> bool {
        self.printing & 1 != 0
    }

    /// `ESC J` has been received: what is received is not shown any more.
    pub fn transparent(&self) -> bool {
        self.transparent & 1 != 0
    }

    /// A cell of the status line: its character, and whether it flashes.
    pub fn status_cell(&self, column: usize) -> (u8, bool) {
        let cell = self.status[column];
        (cell.code, cell.attributes & FLASH != 0)
    }

    /// The status line as text.
    pub fn status_line(&self) -> String {
        self.status.iter().map(|c| c.code as char).collect()
    }

    pub fn switches(&self) -> &Switches {
        &self.switches
    }

    /// Everything the firmware's state is checked by, as lines of text: the
    /// same that `tools/py/d80emu.py` prints of the firmware.  The bytes
    /// sent since the last call are part of it and are taken away.
    pub fn dump(&mut self) -> Vec<String> {
        fn hex(bytes: impl IntoIterator<Item = u8>) -> String {
            bytes.into_iter().map(|b| format!("{:02x}", b)).collect()
        }
        fn cells(cells: &[Cell]) -> String {
            cells
                .iter()
                .map(|c| {
                    format!(
                        "{:X}{:02X}",
                        c.attributes,
                        u8::from(c.protect) << 7 | c.code
                    )
                })
                .collect()
        }
        let pages = self.pages.len();
        let mut out = vec![
            format!(
                "page {} shown {} addressed {} of {}",
                self.page,
                self.port0 & 3,
                self.port0 >> 4 & 3,
                pages
            ),
            format!(
                "bottom {}",
                self.bottom[..pages].iter().map(|b| b.to_string()).collect::<Vec<_>>().join(" ")
            ),
            format!("cursor {} {} status {}", self.cur_row, self.cur_col, self.cur_in_status & 1),
            format!(
                "mode {:02X} latch {:02X} shadow {:02X} port0 {:02X} readback {}",
                self.mode,
                self.latch,
                self.shadow,
                self.port0,
                u8::from(self.readback)
            ),
            format!(
                "tabs {}",
                self.tab_bytes().iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ")
            ),
            format!("lock {} caps {}", self.kbd_locked & 1, self.caps & 1),
            format!("hung {} lost {}", u8::from(self.hung), u8::from(self.fk_lost)),
            format!("bells {}", self.bells),
            format!("sent {}", hex(self.sent.drain(..))),
            format!(
                "work step {:02X} limit {:04X} ptr {:04X} seek {:02X} wrapped {:02X} advance {:02X} \
                 fill {:02X} esc {:02X} {:02X} request {:02X} full {:02X} queue {} keys {}",
                self.step,
                self.limit,
                self.ptr,
                self.seek_state,
                self.wrapped,
                self.advance,
                self.fill,
                self.esc_cnt,
                self.esc_len,
                self.status_req,
                self.q_full,
                self.dispq_in.wrapping_sub(self.dispq_out),
                self.keyq_in.wrapping_sub(self.keyq_out) & 15
            ),
            format!(
                "send {:02X} ptr {:04X} end {:04X} attr {:02X} prot {:02X} all {:02X}",
                self.sending, self.send_ptr, self.send_end, self.send_attr, self.send_prot, self.send_all
            ),
            format!(
                "print {:02X} transparent {:02X} done {:02X} ack {:02X} queued {} ptr {:04X} end {:04X} \
                 page {} prot {:02X} pgm {:02X}",
                self.printing,
                self.transparent,
                self.prn_done,
                self.prn_ack,
                self.prn_queued & 7,
                self.prn_ptr,
                self.prn_end,
                self.prn_page,
                self.prn_prot,
                self.prn_pgm
            ),
            format!(
                "keys recording {:02X} host {:02X} index {:02X} ptr {:04X} free {:04X} memory {}",
                self.fk_recording,
                self.fk_to_host,
                self.fk_index,
                self.fk_ptr,
                self.fk_free,
                hex(self.fk_memory.iter().copied())
            ),
            format!("status {}", cells(&self.status)),
        ];
        for (p, page) in self.pages.iter().enumerate() {
            for (row, line) in page.iter().enumerate() {
                out.push(format!("p{} r{:02} {}", p, row, cells(line)));
            }
        }
        out.push("end".to_string());
        out
    }

    /// The terminal is dead until it is switched off: its firmware goes round
    /// a loop for ever.  `ESC X` does that in protect mode with auto flip,
    /// if the last page has no cell left that is not protected and another
    /// page has one (1F99): clearing a page ends with a search for such a
    /// cell, the search goes on to the other page and stays there, and the
    /// count of the pages cleared starts again from that page.  On the real
    /// terminal the two pages then flicker.  A function key can do it too
    /// (25CF).  The model then takes no more bytes and no more keys, and its
    /// screen is the one the loop keeps coming back to.
    pub fn hung(&self) -> bool {
        self.hung
    }

    /// The model has stopped following the firmware, which has gone on to
    /// read or write memory that is not its function keys' [D80 5.5].  It
    /// does that when a key with a number from 20 to 31 is programmed or
    /// played, when a PROG key is pressed while a key is being programmed,
    /// and some time after its memory for the keys has been full: a key
    /// that ends at the end of that memory is not moved with the others
    /// when room is made, and the next time room is made the sums go wrong.
    /// What the terminal does from then on depends on what its stack holds.
    pub fn lost(&self) -> bool {
        self.fk_lost
    }

    // ------------------------------------------------------------ the main loop

    /// Turn the main loop until a turn changes nothing.
    fn settle(&mut self) {
        for _ in 0..100_000 {
            let before = self.progress();
            self.turn();
            if self.hung || self.progress() == before {
                return;
            }
        }
        debug_assert!(false, "the terminal does not come to rest");
    }

    fn progress(&self) -> Progress {
        let b = u16::from;
        Progress(
            [
                b(self.keyq_in),
                b(self.keyq_out),
                b(self.dispq_in),
                b(self.dispq_out),
                b(self.shadow),
                b(self.port0),
                b(self.mode),
                b(self.status_req),
                b(self.esc_cnt),
                b(self.page),
                b(self.cur_col),
                b(self.cur_row),
                b(self.kbd_locked),
                b(self.printing),
                b(self.transparent),
                b(self.cur_in_status),
                b(self.bottom[0]),
                b(self.bottom[1]),
                b(self.bottom[2]),
                b(self.bottom[3]),
                b(self.send_attr),
                b(self.send_prot),
                self.send_ptr,
                self.send_end,
                b(self.sending),
                b(self.send_all),
                self.prn_ptr,
                self.prn_end,
                b(self.prn_ack),
                b(self.prn_done),
                b(self.prn_queued),
                b(self.step),
                b(self.fk_recording),
                b(self.fk_to_host),
                b(self.fk_index),
                self.fk_ptr,
                b(self.lock_shown),
                b(self.q_full),
                b(self.caps),
                b(self.latch),
            ],
            self.work,
        )
    }

    /// One turn of the firmware's main loop (0115): up to seven keys, what
    /// was received, the status line, and a stretch each of a send, a print
    /// and a function key.
    fn turn(&mut self) {
        for _ in 0..7 {
            if self.keyq_in == self.keyq_out {
                break;
            }
            self.key_dispatch();
        }
        if self.transparent & 1 != 0 {
            self.transparent_pump();
        } else {
            self.display_pump();
        }
        if self.hung {
            return;
        }
        self.status_update();
        self.send_pump();
        self.print_pump();
        self.fk_pump();
    }

    // ------------------------------------------------------------ memory

    fn mode_test(&self, mask: u8) -> u8 {
        if self.mode & mask != 0 {
            0xFF
        } else {
            0
        }
    }

    /// The cell at an address of the page the firmware addresses.  Columns
    /// 80-255 of a row are addresses too, and one routine writes there
    /// (204F, with the cursor in column 0); nothing is behind them here.
    fn locate(&self, addr: u16) -> Option<(usize, usize, usize)> {
        let row = usize::from(high(addr));
        let column = usize::from(low(addr));
        let page = usize::from(self.port0 >> 4 & 3);
        if (0x80..0x98).contains(&row) && column < COLUMNS && page < self.pages.len() {
            Some((page, row - 0x80, column))
        } else {
            None
        }
    }

    fn read(&self, addr: u16) -> u8 {
        match self.locate(addr) {
            Some((page, row, column)) => {
                let cell = self.pages[page][row][column];
                if self.readback {
                    cell.attributes << 3
                } else {
                    u8::from(cell.protect) << 7 | cell.code
                }
            }
            None if (PAGE_RAM..0x9800).contains(&addr) => 0,
            None => 0xFF,
        }
    }

    fn store(cell: &mut Cell, latch: u8, value: u8) {
        if latch & 4 != 0 {
            cell.code = value & 0x7F;
        }
        if latch & 2 != 0 {
            cell.protect = latch & 0x80 != 0;
        }
        if latch & 1 != 0 {
            cell.attributes = latch >> 3 & 0x0F;
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        if let Some((page, row, column)) = self.locate(addr) {
            let cell = &mut self.pages[page][row][column];
            let before = *cell;
            Screen::store(cell, self.latch, value);
            self.work += u64::from(*cell != before);
        }
    }

    fn write_status(&mut self, column: u8, value: u8) {
        if let Some(cell) = self.status.get_mut(usize::from(column)) {
            let before = *cell;
            Screen::store(cell, self.latch, value);
            self.work += u64::from(*cell != before);
        }
    }

    /// Eighty cells from `addr` on.
    fn fill_row(&mut self, addr: u16, value: u8) {
        for i in 0..COLUMNS as u16 {
            self.write(addr.wrapping_add(i), value);
        }
    }

    fn set_latch(&mut self, value: u8) {
        self.latch = value;
    }

    /// The page the firmware addresses (0814).
    fn cpu_page(&mut self, page: u8) {
        self.port0 = self.port0 & 0xCF | (page << 4 & 0x30);
    }

    /// The page shown (0833).
    fn show_page(&mut self, page: u8) {
        self.port0 = self.port0 & 0xFC | (page & 3);
    }

    fn status_dirty(&mut self) {
        self.status_req |= 0x80;
    }

    fn attr_set(&mut self, bits: u8) {
        self.shadow |= bits;
        self.latch = self.shadow;
        self.status_dirty();
    }

    fn attr_clear(&mut self, bits: u8) {
        self.shadow &= !bits;
        self.latch = self.shadow;
        self.status_dirty();
    }

    fn send(&mut self, byte: u8) {
        self.work += 1;
        self.sent.push_back(byte);
    }

    fn bell(&mut self) {
        self.work += 1;
        self.bells += 1;
    }

    // ------------------------------------------------------------ rows and pages

    /// Line of the screen to row of memory, on the current page (13C2).
    fn row_to_mem(&self, line: u8) -> u8 {
        let row = line
            .wrapping_add(self.bottom[usize::from(self.page & 3)])
            .wrapping_add(1);
        if row > 0x17 {
            row.wrapping_sub(0x18)
        } else {
            row
        }
    }

    /// Row of memory to line of the screen (13EA).
    fn mem_to_row(&self, row: u8, page: u8) -> u8 {
        let bottom = self.bottom[usize::from(page & 3)];
        if bottom < row {
            row.wrapping_sub(bottom).wrapping_sub(1)
        } else {
            0x17u8.wrapping_sub(bottom).wrapping_add(row)
        }
    }

    /// The cursor's row back into 0-23: 24 is taken off or added, once
    /// (2992).
    fn row_mod24(&mut self) {
        if self.cur_row < 0x18 {
        } else if self.cur_row < 0x80 {
            self.cur_row -= 0x18;
        } else {
            self.cur_row = self.cur_row.wrapping_add(0x18);
        }
    }

    /// To page + `by`, round the pages either way; the cursor keeps its line
    /// and column (084C).
    fn flip_page(&mut self, by: u8) {
        let mut page = by.wrapping_add(self.page);
        if page > 0x80 {
            page = self.page_max;
        }
        if page > self.page_max {
            page = 0;
        }
        self.cur_row = self.mem_to_row(self.cur_row, self.page);
        self.page = page;
        self.cur_row = self.row_to_mem(self.cur_row);
        self.cpu_page(page);
        self.show_page(page);
        self.status_dirty();
    }

    /// One step of a pointer: 2 if it was at its limit already and did not
    /// move, 1 if it went to another row, else 0 (27A6).  Rows go round the
    /// memory, 23 to 0, not round the screen.
    fn ptr_step(&mut self, walk: Walk) -> u8 {
        let (ptr, limit) = match walk {
            Walk::Send => (self.send_ptr, self.send_end),
            Walk::Print => (self.prn_ptr, self.prn_end),
            Walk::Work => (self.ptr, self.limit),
        };
        if ptr == limit {
            return 2;
        }
        let mut column = low(ptr).wrapping_add(self.step);
        let mut row = high(ptr);
        let mut result = 0;
        if column > 0x4F {
            if self.step < 0x80 {
                column = 0;
                row = row.wrapping_add(1);
            } else {
                column = 0x4F;
                row = row.wrapping_sub(1);
            }
            if row & 0x7F >= 0x18 {
                row = if self.step < 0x80 { 0x80 } else { 0x97 };
            }
            result = 1;
        }
        let moved = u16::from(row) << 8 | u16::from(column);
        match walk {
            Walk::Send => self.send_ptr = moved,
            Walk::Print => self.prn_ptr = moved,
            Walk::Work => self.ptr = moved,
        }
        result
    }

    // ------------------------------------------------------------ receiving

    /// The receive interrupt (2A8E) [D80 2.1].
    fn receive(&mut self, byte: u8) {
        let byte = byte & 0x7F;
        self.rx_byte = byte;
        if byte == 0 && self.mode & PGM == 0 {
            return;
        }
        if byte == 0x1B || self.esc_cnt != 0 {
            self.esc_collect();
        } else {
            self.dispq_put();
        }
    }

    /// The collector (06E7): the byte behind ESC says how many more belong to
    /// the sequence and gets bit 7; when all are there they go into the queue
    /// together.  Nothing looks at what a parameter is.
    fn esc_collect(&mut self) {
        if self.esc_cnt == 0 {
            self.esc_cnt = 0xFF;
            return;
        }
        if self.esc_cnt == 0xFF {
            self.esc_cnt = match self.rx_byte {
                b'G' => 2,
                b'f' => 5,
                b'=' | b'e' => 3,
                _ => 1,
            };
            self.esc_len = self.esc_cnt;
            self.rx_byte |= 0x80;
        }
        self.esc_cnt -= 1;
        self.esc_buf[usize::from(self.esc_cnt)] = self.rx_byte;
        if self.esc_cnt == 0 {
            self.esc_cnt = self.esc_len;
            while self.esc_cnt != 0 {
                self.rx_byte = self.esc_buf[usize::from(self.esc_cnt) - 1];
                self.dispq_put();
                self.esc_cnt -= 1;
            }
        }
    }

    /// Into the queue (2A61).  If it is full the byte is lost and the status
    /// line gets COMM ERR:O, for which whatever else was to be shown there is
    /// forgotten.
    fn dispq_put(&mut self) {
        self.dispq_in = self.dispq_in.wrapping_add(1);
        if self.dispq_out == self.dispq_in {
            self.dispq_in = self.dispq_in.wrapping_sub(1);
            self.status_req = 0x10;
            return;
        }
        self.work += 1;
        self.dispq[usize::from(self.dispq_in)] = self.rx_byte;
    }

    /// Out of the queue (2A46).  A function key being programmed gets it too.
    fn dispq_get(&mut self) -> u8 {
        self.dispq_out = self.dispq_out.wrapping_add(1);
        self.cur_byte = self.dispq[usize::from(self.dispq_out)];
        if self.mode & PGK != 0 {
            self.fk_capture();
        }
        self.cur_byte
    }

    /// Work off the queue (29A8).
    fn display_pump(&mut self) {
        let mut taken = 0u32;
        while self.dispq_in != self.dispq_out {
            // Nothing puts into the queue what would make this go round for
            // ever, but nothing in the firmware would stop it either.
            taken += 1;
            if taken > 1_000_000 {
                self.hung = true;
                return;
            }
            let mut byte = self.dispq_get();
            let mut program = self.mode & PGM != 0;
            loop {
                if program {
                    // program mode [D80 2.9]: everything is shown, but for a
                    // few escape functions
                    if (0x1B..0x80).contains(&byte) {
                    } else if byte < 0x1B {
                        byte = CONTROL_PICTURE[usize::from(byte)];
                    } else {
                        let letter = byte & 0x7F;
                        if letter >= 0x20 && works_in_program_mode(letter) {
                            program = false;
                            continue;
                        }
                        // the picture of ESC, then the letter as a character
                        self.put_char(0x1B);
                        self.cursor_right();
                        byte = letter;
                        continue;
                    }
                } else if byte < 0x20 {
                    self.control(byte);
                    break;
                } else if byte == 0x7F {
                    // DEL is nothing, and what is behind it in the queue
                    // waits for the next turn (29C6)
                    return;
                } else if byte > 0x7F {
                    self.escape(byte);
                    break;
                }
                self.put_char(byte);
                self.advance = 0xFF;
                break;
            }
            if self.hung {
                return;
            }
            if self.advance != 0 {
                self.cursor_right();
            }
        }
    }

    /// A character into the cell under the cursor (2A3A).
    fn put_char(&mut self, byte: u8) {
        self.write(
            u16::from(self.cur_row | 0x80) << 8 | u16::from(self.cur_col),
            byte,
        );
    }

    // ------------------------------------------------------------ the cursor

    /// The cursor one to the right behind a character (296B).
    fn cursor_right(&mut self) {
        let next = self.cur_col.wrapping_add(1);
        if self.mode & PRT == 0 && next < 0x50 && next != self.switches.bell_column {
            self.cur_col = next;
        } else {
            self.move_cursor(1, 0);
        }
    }

    /// The cursor by `columns` and `lines`, each 0, +1 or -1 (28B4)
    /// [D80 2.3].
    fn move_cursor(&mut self, columns: u8, mut lines: u8) {
        let column = self.cur_col.wrapping_add(columns);
        self.cur_col = column;
        if column >= 0x80 {
            // left of column 0: the end of the line above, always
            self.cur_col = 0x4F;
            lines = lines.wrapping_sub(1);
        } else {
            if column >= 0x50 {
                if !self.switches.wrap {
                    self.cur_col = 0x4F;
                    return;
                }
                self.cur_col = 0;
                lines = lines.wrapping_add(1);
            }
            if columns == 1 && column == self.switches.bell_column {
                self.bell();
            }
        }
        if lines == 0 {
            self.prot_seek(0, columns);
            return;
        }
        let old = self.cur_row;
        self.cur_row = old.wrapping_add(lines);
        self.row_mod24();
        let bottom = self.bottom[usize::from(self.page)];
        if self.mode & FLP != 0 {
            if lines < 0x80 {
                if old == bottom {
                    self.flip_page(1);
                }
            } else if bottom == self.cur_row {
                self.flip_page(0xFF);
            }
        }
        self.prot_seek(0, lines);
        if bottom == old && lines < 0x80 {
            self.scroll_new_line();
        }
    }

    /// The cursor has gone down from the last line and is on the row that
    /// was the first.  In conversation mode that row is filled with blanks
    /// and becomes the last line: the screen has scrolled.  In block mode,
    /// protect mode and with auto flip nothing more happens, so the cursor
    /// is on the first line (08A9).  The blanks are written like any
    /// character: the row keeps the attributes it had as the first line.
    fn scroll_new_line(&mut self) {
        if self.mode & (FLP | BLK | PRT) != 0 {
            return;
        }
        self.fill_row(cell_addr(self.cur_row, 0), b' ');
        self.bottom[usize::from(self.page)] = self.cur_row;
    }

    fn home(&mut self) {
        self.cur_col = 0;
        self.cur_row = self.bottom[usize::from(self.page)].wrapping_add(1);
        self.row_mod24();
    }

    /// In protect mode: from the cursor on, forward or backward (`direction`
    /// +1 or -1), to the first cell whose protect bit is as bit 7 of `want`;
    /// 0 looks for a cell that is not protected (1D79) [D80 2.7].  At the end
    /// of the page the walk goes on from the other end (of the next page,
    /// with auto flip) and from then on looks for an unprotected cell.  If
    /// the pages have all been gone through: PROT ERR.
    fn prot_seek(&mut self, want: u8, direction: u8) {
        self.seek_want = want;
        if self.mode & PRT == 0 {
            return;
        }
        self.step = direction;
        let mut pages_done = 0xFFu8;
        self.wrapped = 0;
        self.ptr = cell_addr(self.cur_row, self.cur_col);
        self.limit = if self.step < 0x80 {
            cell_addr(self.bottom[usize::from(self.page)], 0x4F)
        } else {
            cell_addr(self.row_to_mem(0), 0)
        };
        for _ in 0..1_000_000 {
            if self.seek_state == 2 {
                self.wrapped = 1;
                self.seek_want = 0;
                pages_done = pages_done.wrapping_add(1);
                if pages_done > self.page_max {
                    self.status_req |= PROT_ERR;
                    self.cur_in_status = 0xFF;
                    self.cur_col = 0;
                    self.seek_state = 0;
                    return;
                }
                if self.mode & FLP != 0 {
                    self.flip_page(self.step);
                    self.limit = cell_addr(self.cur_row, self.cur_col);
                }
                self.ptr = if self.step < 0x80 {
                    cell_addr(self.row_to_mem(0), 0)
                } else {
                    cell_addr(self.bottom[usize::from(self.page)], 0x4F)
                };
            }
            if (self.seek_want ^ self.read(self.ptr)) < 0x80 {
                break;
            }
            self.seek_state = self.ptr_step(Walk::Work);
        }
        self.cur_col = low(self.ptr);
        self.cur_row = high(self.ptr) & 0x7F;
        self.seek_state = 0;
    }

    // ------------------------------------------------------------ control codes

    /// A control code (08D3) [D80 2.4].
    fn control(&mut self, code: u8) {
        self.advance = 0;
        match code & 0x1F {
            0x07 => self.bell(),
            0x08 => self.move_cursor(0xFF, 0),
            0x09 => {
                if self.mode & PRT != 0 {
                    self.tab_field();
                } else {
                    self.tab_column();
                }
            }
            0x0A => self.move_cursor(0, 1),
            0x0B => self.move_cursor(0, 0xFF),
            // cursor right, as behind a character
            0x0C => self.advance = 1,
            0x0D => {
                self.cur_col = 0;
                self.prot_seek(0, 1);
            }
            // the whole page to blanks, protected cells too
            0x1A => {
                self.fill = b' ';
                self.skip_prot = 0;
                self.erase_page(0xFF);
            }
            0x1E => {
                self.home();
                self.prot_seek(0, 1);
            }
            0x1F => {
                self.cur_col = 0;
                self.move_cursor(0, 1);
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------ escape functions

    /// An escape function, by its letter (09A3) [D80 2.5].  ESC and a control
    /// code is nothing at all, not even the end of the cursor's move behind
    /// the byte before: if that was a character, the cursor moves once more
    /// (09B8) [D80 2.8].
    fn escape(&mut self, code: u8) {
        let letter = code & 0x7F;
        if letter < 0x20 {
            return;
        }
        self.advance = 0;
        match letter {
            b'"' => {
                self.kbd_locked = 0;
                self.status_dirty();
            }
            b'#' => {
                self.kbd_locked = 0xFF;
                self.status_dirty();
            }
            b'&' => {
                if self.mode & PGM != 0 {
                    self.status_req |= MODE_ERR;
                } else {
                    self.mode |= PRT;
                    self.prot_seek(0, 1);
                    self.status_dirty();
                }
            }
            b'\'' => {
                self.mode &= !PRT;
                self.status_dirty();
            }
            b'(' => {
                self.mode &= !WPT;
                self.attr_clear(0x80);
            }
            b')' => {
                if self.mode & PGM != 0 {
                    self.status_req |= MODE_ERR;
                } else {
                    self.mode |= WPT;
                    self.attr_set(0x82);
                }
            }
            b'*' => self.erase(0, 0, 0xFF),
            b'+' | b'Z' => self.erase(b' ', 0, 0xFF),
            b'1' | b'2' => {
                let set = if letter == b'1' { 0xFF } else { 0 };
                if self.mode & PRT != 0 {
                    self.column_protect(set);
                } else {
                    self.tab_set_clear(set);
                }
            }
            b'3' => {
                if self.mode & PRT == 0 {
                    self.tabs = [0; 9];
                    self.skip_prot = 0;
                }
            }
            b'4' => self.send_start(0),
            b'5' => self.send_start(1),
            b'6' => {
                self.send_start(0);
                self.send_all = 0xFF;
            }
            b'7' => {
                self.send_start(1);
                self.send_all = 0xFF;
            }
            b':' => self.erase(0, 0xFF, 0xFF),
            b';' => self.erase(b' ', 0xFF, 0xFF),
            b'=' => self.cursor_address(),
            b'?' => {
                let line = self.mem_to_row(self.cur_row, self.page);
                self.send(line.wrapping_add(0x20));
                self.send(self.cur_col.wrapping_add(0x20));
            }
            b'A' => {
                self.mode |= ATB;
                self.attr_set(0x01);
            }
            b'B' => {
                self.mode |= BLK;
                self.status_dirty();
            }
            b'C' => {
                self.mode &= !BLK;
                self.status_dirty();
            }
            b'E' => self.insert_line(),
            b'F' => {
                self.flip_page(self.page_max.wrapping_sub(self.page).wrapping_add(1));
                self.prot_seek(0, 1);
            }
            b'G' => self.line_drawing(),
            // back tab: in protect mode only
            b'I' => {
                if self.mode & PRT != 0 {
                    self.back_tab();
                }
            }
            b'J' => {
                self.transparent = 0xFF;
                self.printing = 0xFF;
                self.prn_done = 0;
                self.status_dirty();
            }
            b'N' => {
                self.flip_page(1);
                self.prot_seek(0, 1);
            }
            b'O' => self.prn_done = 0xFF,
            b'P' => self.print_start(),
            b'Q' => {
                self.ch_step = 1;
                self.ch_back = 0xFFFE;
                self.ins_del_char();
            }
            b'R' => self.delete_line(),
            b'T' | b't' => {
                self.ptr = cell_addr(self.cur_row, self.cur_col);
                self.fill = if letter == b'T' { b' ' } else { 0 };
                self.skip_prot = self.mode_test(PRT);
                self.erase_eol();
                self.prot_seek(0, 1);
            }
            b'W' => {
                self.ch_step = 0xFFFF;
                self.ch_back = 2;
                self.ins_del_char();
            }
            b'X' => self.clear_all_pages(),
            b'Y' => self.erase(b' ', 0xFF, 0),
            b'a' => {
                self.mode &= !ATB;
                self.attr_clear(0x01);
            }
            b'c' => {
                if self.mode & (PRT | WPT | ATB) != 0 {
                    self.status_req |= MODE_ERR;
                } else {
                    self.port0 |= 0x08;
                    self.mode |= PGM;
                    self.status_dirty();
                }
            }
            b'd' => {
                self.mode &= !PGM;
                self.port0 &= !0x08;
                self.status_dirty();
            }
            b'e' => self.fk_program(),
            b'f' => self.set_field(),
            b'i' => self.control(0x09),
            b'j' => self.attr_set(0x08),
            b'k' => self.attr_clear(0x08),
            b'l' => self.attr_set(0x40),
            b'm' => self.attr_clear(0x40),
            b'n' => self.attr_set(0x20),
            b'o' => self.attr_clear(0x20),
            b'p' => self.attr_set(0x10),
            b'q' => self.attr_clear(0x10),
            b'v' => {
                self.mode |= FLP;
                self.status_dirty();
            }
            b'w' => {
                self.mode &= !FLP;
                self.status_dirty();
            }
            b'y' => self.erase(0, 0xFF, 0),
            // S, V and x mean something inside a function key only
            _ => {}
        }
    }

    /// `ESC = row column` (0B1C) [D80 2.6.1].  The two bytes are taken from
    /// the queue as they are.  The row is checked only after `bottom` has
    /// been added and 24 taken off once: on a screen that has not scrolled,
    /// rows 0-23 are right and 24 and more are CURS ERR, but row bytes below
    /// 20h are taken as well; on one that has scrolled, some rows above 23
    /// are.
    fn cursor_address(&mut self) {
        self.dispq_out = self.dispq_out.wrapping_add(1);
        let row = self.dispq[usize::from(self.dispq_out)];
        self.cur_row = row
            .wrapping_add(self.bottom[usize::from(self.page)])
            .wrapping_sub(0x1F);
        self.row_mod24();
        self.dispq_out = self.dispq_out.wrapping_add(1);
        let column = self.dispq[usize::from(self.dispq_out)];
        self.cur_col = column.wrapping_sub(0x20);
        self.cur_in_status = 0;
        if self.cur_col > 0x4F || self.cur_row > 0x17 {
            self.status_req |= CURS_ERR;
            self.cur_row = 0;
            self.cur_col = 0;
            self.cur_in_status = 0xFF;
        }
        self.row_mod24();
        self.prot_seek(0, 1);
    }

    /// `ESC G x`: x from A to K is one of the eleven line-drawing characters
    /// (15B0).
    fn line_drawing(&mut self) {
        let which = self.dispq_get().wrapping_sub(b'A');
        if which > 0x0A {
            return;
        }
        self.ptr = cell_addr(self.cur_row, self.cur_col);
        self.write(self.ptr, which + 1);
        self.advance = 0xFF;
    }

    /// `ESC f r1 c1 r2 c2`: the protect bit, and in write attribute mode the
    /// attributes, of every cell from the one to the other; the characters
    /// stay (14B9) [D80 2.6.7].
    fn set_field(&mut self) {
        let r1 = self.dispq_get().wrapping_sub(0x20);
        let c1 = self.dispq_get().wrapping_sub(0x20);
        let r2 = self.dispq_get().wrapping_sub(0x20);
        let c2 = self.dispq_get().wrapping_sub(0x20);
        if self.mode & (ATB | WPT) == 0 {
            return;
        }
        if (r1 == r2 && c2 < c1) || r2 < r1 {
            return;
        }
        if r1 > 0x17 || r2 > 0x17 || c1 > 0x4F || c2 > 0x4F {
            return;
        }
        let r2 = self.row_to_mem(r2);
        let r1 = self.row_to_mem(r1);
        self.set_latch(self.shadow & 0xFB);
        self.ptr = cell_addr(r1, c1);
        self.limit = cell_addr(r2, c2);
        self.step = 1;
        for _ in 0..4 * LINES * COLUMNS {
            self.write(self.ptr, 0);
            if self.ptr_step(Walk::Work) == 2 {
                break;
            }
        }
        self.set_latch(self.shadow);
        self.prot_seek(0, 1);
    }

    // ------------------------------------------------------------ erasing

    fn erase(&mut self, fill: u8, skip_prot: u8, whole: u8) {
        self.fill = fill;
        self.skip_prot = skip_prot;
        self.erase_page(whole);
    }

    /// From the pointer to the end of its line (1E97).
    fn erase_eol(&mut self) {
        let mut column = low(self.ptr);
        while column <= 0x4F {
            if self.skip_prot & 1 == 0 || self.read(self.ptr) < 0x80 {
                self.write(self.ptr, self.fill);
            }
            self.ptr = self.ptr.wrapping_add(1);
            column += 1;
        }
    }

    /// From the cursor, or with `whole` from home, to the end of the page
    /// (1ED7) [D80 2.6.2].  If protected cells are not to be kept everything
    /// goes: characters, protect bits, attributes, and the attributes that
    /// are switched on are switched off.  If they are to be kept they are
    /// kept in protect mode only, and the cells are written like characters.
    fn erase_page(&mut self, whole: u8) {
        if whole & 1 != 0 {
            self.home();
            if self.skip_prot & 1 == 0 {
                self.set_latch(self.shadow & 0x7F);
            }
        }
        self.ptr = cell_addr(self.cur_row, self.cur_col);
        let all = !self.skip_prot;
        if all & 1 != 0 {
            self.set_latch(0x07);
        } else {
            self.skip_prot &= self.mode_test(PRT);
        }
        let last = self.mem_to_row(0x17, self.page);
        let mut line = self.mem_to_row(self.cur_row, self.page);
        while line <= 0x17 {
            self.erase_eol();
            self.ptr = self.ptr.wrapping_add(0xB0);
            if line == last {
                self.ptr = PAGE_RAM;
            }
            line += 1;
        }
        if all & 1 != 0 {
            self.attr_clear(0x78);
        } else {
            self.set_latch(self.shadow);
        }
        self.prot_seek(0, 1);
    }

    /// `ESC X`, and switching on: every page (1F86).
    fn clear_all_pages(&mut self) {
        self.fill = 0;
        self.skip_prot = 0xFF;
        let saved = self.page;
        self.page = 0;
        let mut rounds = 0;
        while self.page <= self.page_max {
            // The search at the end of clearing a page can leave another
            // page current, and the count goes on from that one: back to an
            // earlier page means round and round for ever.
            rounds += 1;
            if rounds > 16 {
                self.hung = true;
                return;
            }
            self.cpu_page(self.page);
            self.set_latch(0x07);
            self.erase_page(0xFF);
            self.page = self.page.wrapping_add(1);
        }
        self.page = saved;
        self.cpu_page(saved);
        self.attr_clear(0x78);
        self.control(0x1E);
    }

    // ------------------------------------------------------------ tab stops

    fn tab_byte(&mut self, index: u8) -> &mut u8 {
        match self.tabs.get_mut(usize::from(index)) {
            Some(byte) => byte,
            None => &mut self.skip_prot,
        }
    }

    /// `ESC 1` and `ESC 2` outside protect mode (1FF7).
    fn tab_set_clear(&mut self, set: u8) {
        let mask = 1u8 << (self.cur_col & 7);
        let byte = self.tab_byte(self.cur_col >> 3);
        if set & 1 != 0 {
            *byte |= mask;
        } else {
            *byte &= !mask;
        }
    }

    /// HT outside protect mode: to the next stop on the line; if there is
    /// none the cursor stays (214C).
    fn tab_column(&mut self) {
        let next = self.cur_col.wrapping_add(1);
        let mut bit = next & 7;
        let mut index = next >> 3;
        while index <= 9 {
            let mut stops = *self.tab_byte(index) >> bit;
            while stops != 0 {
                if stops & 1 != 0 {
                    self.cur_col = index * 8 + bit;
                    self.ptr = cell_addr(self.cur_row, self.cur_col);
                    return;
                }
                stops >>= 1;
                bit += 1;
            }
            bit = 0;
            index += 1;
        }
    }

    /// `ESC 1` and `ESC 2` in protect mode: the protect bit of the column
    /// left of the cursor, from the cursor's line to the last (204F).  Only
    /// the protect bit is written.  The step to the left uses the limit some
    /// other routine left behind: if the cursor happens to be on it, the
    /// cursor's own column is taken.
    fn column_protect(&mut self, set: u8) {
        self.set_latch((set & 1) << 7 | 0x02);
        self.ptr = cell_addr(self.cur_row, self.cur_col);
        self.step = 0xFF;
        self.ptr_step(Walk::Work);
        let last = self.mem_to_row(0x17, self.page);
        let mut line = self.mem_to_row(high(self.ptr) & 0x7F, self.page);
        while line <= 0x17 {
            self.write(self.ptr, set);
            self.ptr = self.ptr.wrapping_add(0x100);
            if line == last {
                self.ptr = cell_addr(0, self.cur_col.wrapping_sub(1));
            }
            line += 1;
        }
        self.set_latch(self.shadow);
    }

    /// `ESC I` in protect mode: to the start of the field before (20DC).
    fn back_tab(&mut self) {
        self.step = 0xFF;
        self.ptr = cell_addr(self.cur_row, self.cur_col);
        self.limit = cell_addr(self.row_to_mem(0), 0);
        self.seek_state = self.ptr_step(Walk::Work);
        self.cur_row = high(self.ptr) & 0x7F;
        self.cur_col = low(self.ptr);
        self.prot_seek(0, 0xFF);
        if self.wrapped & 1 != 0 {
            return;
        }
        self.prot_seek(0x80, 0xFF);
        if self.wrapped & 1 != 0 {
            return;
        }
        self.prot_seek(0, 1);
    }

    /// HT in protect mode: to the start of the next field (2136).
    fn tab_field(&mut self) {
        self.prot_seek(0x80, 1);
        if self.wrapped & 1 == 0 {
            self.prot_seek(0, 1);
        }
    }

    // ------------------------------------------------------------ insert and delete

    /// `ESC Q` and `ESC W` (19C3) [D80 2.6.3]: to the end of the line, in
    /// protect mode to the end of the field.  A cell moves with its protect
    /// bit and its attributes; the blank is written like a character.
    fn ins_del_char(&mut self) {
        if self.cur_in_status & 1 != 0 {
            return;
        }
        let mut column = self.cur_col;
        let mut end = if self.mode & PRT != 0 {
            self.field_end(column)
        } else {
            0x50
        };
        let mut count = end.wrapping_sub(column);
        if count == 0 {
            return;
        }
        end = end.wrapping_sub(1);
        if self.ch_step == 1 {
            column = end;
        }
        self.ptr = cell_addr(self.cur_row, column).wrapping_sub(self.ch_step);
        while count > 1 {
            let character = self.read(self.ptr);
            self.readback = true;
            let latch = self.read(self.ptr) & 0x78 | 0x07 | character & 0x80;
            self.set_latch(latch);
            self.readback = false;
            self.ptr = self.ptr.wrapping_add(self.ch_step);
            self.write(self.ptr, character);
            self.ptr = self.ptr.wrapping_add(self.ch_back);
            count -= 1;
        }
        self.set_latch(self.shadow);
        self.ptr = self.ptr.wrapping_add(self.ch_step);
        self.write(self.ptr, b' ');
        self.prot_seek(0, 1);
    }

    /// The column of the first protected cell from `column` on, or 80
    /// (1988).
    fn field_end(&mut self, mut column: u8) -> u8 {
        self.ptr = cell_addr(self.cur_row, column);
        while column <= 0x4F {
            if self.read(self.ptr) > 0x7F {
                return column;
            }
            self.ptr = self.ptr.wrapping_add(1);
            column += 1;
        }
        0x50
    }

    /// The pages a line insert or delete goes through: with auto flip, all
    /// from the current one on (1AB2).
    fn page_span(&mut self) {
        self.pg_last = if self.mode & FLP != 0 {
            self.page_max
        } else {
            self.page
        };
    }

    /// A page's first line becomes its last (`delta` 1) or the other way
    /// round (-1) (1ACB).
    fn roll_page(&mut self, page: u8) {
        self.row_mod24();
        let bottom = &mut self.bottom[usize::from(page & 3)];
        let moved = self.delta.wrapping_add(*bottom);
        *bottom = if moved > 0x7F { 0x17 } else { moved % 0x18 };
    }

    /// A line to another, of the same or another page, cells whole (1B33).
    fn copy_row(&mut self) {
        let saved = self.page;
        self.page = self.dst_page;
        let to = usize::from(self.row_to_mem(self.dst_row));
        self.page = self.src_page;
        let from = usize::from(self.row_to_mem(self.src_row));
        self.page = saved;
        let (src, dst) = (
            usize::from(self.src_page & 3),
            usize::from(self.dst_page & 3),
        );
        if from < LINES && to < LINES && src < self.pages.len() && dst < self.pages.len() {
            let row = self.pages[src][from];
            self.work += u64::from(self.pages[dst][to] != row);
            self.pages[dst][to] = row;
        }
        self.cpu_page(self.page);
        self.set_latch(self.shadow);
    }

    /// The lines above or below the cursor's, one up or down (1BA1).  With
    /// the cursor in the upper half of the screen the page is rolled and the
    /// lines above the cursor are moved; in the lower half the lines below.
    fn shift_rows(&mut self) {
        self.cur_lrow = self.mem_to_row(self.cur_row, self.page);
        self.dst_page = self.page;
        self.src_page = self.page;
        let line = self.cur_lrow;
        if line < 0x0C {
            if self.line_dir == 1 {
                self.cur_row = self.cur_row.wrapping_sub(1);
                self.delta = 0xFF;
                self.roll_page(self.page);
            }
            for count in (1..=line).rev() {
                if self.line_dir == 1 {
                    self.src_row = line + 1 - count;
                    self.dst_row = self.src_row - 1;
                } else {
                    self.src_row = count - 1;
                    self.dst_row = count;
                }
                self.copy_row();
            }
            if self.line_dir == 0xFF {
                self.cur_row = self.cur_row.wrapping_add(1);
                self.delta = 1;
                self.roll_page(self.page);
            }
        } else {
            for count in (1..=0x17 - line).rev() {
                if self.line_dir == 1 {
                    self.src_row = count + line - 1;
                    self.dst_row = self.src_row + 1;
                } else {
                    self.src_row = 0x18 - count;
                    self.dst_row = self.src_row - 1;
                }
                self.copy_row();
            }
        }
    }

    /// `ESC E` (1C5A) [D80 2.6.4].  Not in protect mode.  The cursor stays
    /// where it is.  With auto flip the last line of each page goes to the
    /// top of the next.  The new line is blanks over a copy of the line that
    /// was the cursor's: it has that line's attributes.
    fn insert_line(&mut self) {
        if self.mode & PRT != 0 {
            return;
        }
        self.page_span();
        self.src_row = 0x17;
        self.dst_row = 0;
        self.dst_page = self.pg_last;
        self.src_page = self.pg_last.wrapping_sub(1);
        self.delta = 0xFF;
        while self.dst_page != self.page {
            self.roll_page(self.dst_page);
            self.copy_row();
            self.dst_page = self.dst_page.wrapping_sub(1);
            self.src_page = self.dst_page.wrapping_sub(1);
        }
        self.line_dir = 1;
        self.shift_rows();
        if self.cur_lrow < 0x0C {
            self.dst_row = self.cur_lrow;
            self.src_row = self.cur_lrow + 1;
            self.src_page = self.page;
            self.dst_page = self.page;
            self.copy_row();
        }
        self.fill_row(cell_addr(self.cur_row, 0), b' ');
    }

    /// `ESC R` (1CD6).  Not in protect mode.  With auto flip the first line
    /// of each following page comes up to the end of the page before.  The
    /// line that comes free is blanks over a copy of the line above it.
    fn delete_line(&mut self) {
        if self.mode & PRT != 0 {
            return;
        }
        self.page_span();
        self.line_dir = 0xFF;
        self.shift_rows();
        self.src_row = 0;
        self.dst_row = 0x17;
        self.dst_page = self.page;
        self.src_page = self.page.wrapping_add(1);
        self.delta = 1;
        while self.dst_page != self.pg_last {
            self.copy_row();
            self.roll_page(self.src_page);
            self.dst_page = self.dst_page.wrapping_add(1);
            self.src_page = self.src_page.wrapping_add(1);
        }
        self.src_row = 0x16;
        self.src_page = self.pg_last;
        self.dst_page = self.pg_last;
        self.copy_row();
        self.cpu_page(self.dst_page);
        self.fill_row(
            cell_addr(self.bottom[usize::from(self.dst_page & 3)], 0),
            b' ',
        );
        self.cpu_page(self.page);
    }

    // ------------------------------------------------------------ sending

    /// `ESC 4` to `ESC 7` (15DE) [D80 4.3]: from the start of the cursor's
    /// line, or with `page` from home, up to the cursor and with it.
    fn send_start(&mut self, page: u8) {
        self.send_end = cell_addr(self.cur_row, self.cur_col);
        self.send_attr = 0;
        self.send_prot = 0;
        self.send_all = 0;
        self.send_ptr = if page & 1 != 0 {
            cell_addr(self.row_to_mem(0), 0)
        } else {
            cell_addr(self.cur_row, 0)
        };
        self.sending = 0xFF;
    }

    /// A stretch of a send: fifteen cells, and none while more than two keys
    /// wait (172B).  Empty cells are not sent, but in program mode; US is
    /// sent at the end of each line and CR at the end, not in program mode.
    fn send_pump(&mut self) {
        for _ in 0..15 {
            if self.keyq_in.wrapping_sub(self.keyq_out) & 0x0F > 2 || self.sending & 1 == 0 {
                return;
            }
            if self.mode & ATB != 0 {
                self.send_attr_edge();
            }
            if self.mode & PRT != 0 {
                self.send_prot_edge();
            }
            let cell = self.read(self.send_ptr);
            if self.mode & PGM != 0 || cell & 0x7F != 0 {
                self.send(Screen::screen_to_ascii(cell));
            }
            self.step = 1;
            match self.ptr_step(Walk::Send) {
                1 => self.send(0x1F),
                2 => {
                    if self.mode & PGM == 0 {
                        self.send(0x0D);
                    }
                    self.sending = 0;
                }
                _ => {}
            }
        }
    }

    /// In protect mode, where protected cells begin or end (1627): `ESC )`
    /// and `ESC (` if all is sent; else FS, and the protected cells are
    /// walked over.  That walk goes the way the last walk of anything went:
    /// backward, if the cursor was last moved backward.
    fn send_prot_edge(&mut self) {
        if self.read(self.send_ptr) & 0x80 == self.send_prot {
            return;
        }
        if self.send_all & 1 != 0 {
            self.send(0x1B);
            self.send(if self.send_prot != 0 { b'(' } else { b')' });
        } else {
            if self.send_prot == 0 {
                self.send(0x1C);
            }
            for _ in 0..4 * LINES * COLUMNS {
                if self.read(self.send_ptr) <= 0x7F {
                    break;
                }
                self.send_prot = self.ptr_step(Walk::Send);
                if self.send_prot == 2 {
                    return;
                }
                if self.send_prot == 1 {
                    self.send(0x1F);
                }
            }
        }
        self.send_prot = self.read(self.send_ptr) & 0x80;
    }

    /// In write attribute mode, where the attributes change: ESC and the
    /// letter that switches each one on or off (1699).
    fn send_attr_edge(&mut self) {
        self.readback = true;
        let now = self.read(self.send_ptr) & 0x78;
        if now != self.send_attr {
            let mut changed = (self.send_attr ^ now) >> 3;
            let mut before = self.send_attr >> 3;
            let mut letter = 0;
            while changed != 0 {
                if changed & 1 != 0 {
                    self.send(0x1B);
                    if before & 1 != 0 {
                        letter += 1;
                    }
                    self.send(ATTRIBUTE_LETTERS[letter & 7]);
                }
                letter = (letter & 0x0E) + 2;
                changed >>= 1;
                before >>= 1;
            }
        }
        self.send_attr = self.read(self.send_ptr) & 0x78;
        self.readback = false;
    }

    /// What is sent for a cell: the codes 00-1A go back to the control code
    /// they are the picture of (1478).
    fn screen_to_ascii(cell: u8) -> u8 {
        let code = cell & 0x7F;
        match CONTROL_PICTURE.iter().position(|&c| c == code) {
            Some(control) => control as u8,
            None => code,
        }
    }

    // ------------------------------------------------------------ printing

    /// `ESC P` (18A9): from home to the cursor.
    fn print_start(&mut self) {
        self.prn_prot = self.mode_test(PRT);
        self.prn_pgm = self.mode_test(PGM);
        self.prn_end = cell_addr(self.cur_row, self.cur_col);
        self.prn_ptr = cell_addr(self.row_to_mem(0), 0);
        self.prn_page = self.page;
        self.prn_ack = 0xFF;
        self.printing = 0xFF;
        self.prn_done = 0;
        self.status_dirty();
    }

    fn prn_put(&mut self) {
        self.work += 1;
        self.prn_queued = self.prn_queued.wrapping_add(1);
    }

    /// A stretch of a print (17C7): cells go into the printer's queue while
    /// it holds no more than two bytes.  The queue is never emptied here, so
    /// a print gets as far as its first three characters or its first line
    /// end; if it reaches its end before that, CR is sent to the host; if
    /// not, it stays where it is for good.  Either way the terminal keeps
    /// printing in its status line, because the queue is not empty.
    fn print_pump(&mut self) {
        if self.printing & 1 == 0 || self.transparent & 1 != 0 {
            return;
        }
        if self.prn_queued & 7 == 0 && self.prn_done & 1 != 0 {
            self.printing = 0;
            self.status_dirty();
            return;
        }
        self.step = 1;
        while self.prn_queued & 7 <= 2 && self.prn_done & 1 == 0 {
            self.cpu_page(self.prn_page);
            let cell = self.read(self.prn_ptr);
            let character = if cell > 0x7F && self.prn_prot & 1 != 0 {
                b' '
            } else {
                cell
            };
            if self.mode & PGM != 0 || character & 0x7F != 0 {
                self.prn_put();
            }
            let stepped = self.ptr_step(Walk::Print);
            if stepped != 0 {
                // CR, LF, NUL
                self.prn_put();
                self.prn_put();
                self.prn_put();
            }
            if stepped == 2 {
                self.prn_done = 0xFF;
                if !self.prn_pgm & self.prn_ack & 1 != 0 {
                    self.send(0x0D);
                }
            }
            self.cpu_page(self.page);
        }
    }

    /// In place of showing what was received, behind `ESC J` (1903): it goes
    /// to the printer's queue while that holds no more than two bytes, an
    /// escape function as two.  `ESC K` would end it once the queue is empty,
    /// which here it never is: the terminal takes three characters at most
    /// and then no more, and what is received stays in the queue of 256
    /// until that is full.
    fn transparent_pump(&mut self) {
        if self.prn_queued & 7 == 0 && self.prn_done & 1 != 0 {
            self.printing = 0;
            self.transparent = 0;
            self.status_dirty();
        }
        while self.dispq_in != self.dispq_out && self.prn_queued & 7 <= 2 {
            let byte = self.dispq_get();
            if byte == 0xCB {
                self.prn_done = 0xFF;
            } else if byte > 0x7F {
                self.prn_put();
            }
            self.prn_put();
        }
    }

    // ------------------------------------------------------------ the status line

    /// Text into the status line from `column` on.
    fn status_text(&mut self, column: u8, text: &[u8]) {
        for (i, &c) in text.iter().enumerate() {
            self.write_status(column.wrapping_add(i as u8), c);
        }
    }

    /// For each bit of `mask` that is set, from bit 0 up, the next item of
    /// `text`, one behind the other from `column` on (026F).
    fn status_items(&mut self, mut column: u8, mut mask: u8, text: &[u8], length: usize) -> u8 {
        let mut item = 0;
        while mask != 0 {
            if mask & 1 != 0 {
                self.status_text(column, &text[item..item + length]);
                column = column.wrapping_add(length as u8);
            }
            item += length;
            mask >>= 1;
        }
        column
    }

    /// Write what `status_req` asks for into the status line (02C3) [D80 6].
    fn status_update(&mut self) {
        let lock = if self.switches.locked { 4 } else { 0 };
        if lock != self.lock_shown {
            self.status_dirty();
        }
        if self.status_req == 0 {
            return;
        }
        let request = self.status_req;
        self.set_latch(0x27);
        if request & 0x70 != 0 {
            self.status_text(0, b"COMM ERR:");
        } else if request & 0x0F == 1 {
            self.status_text(0, &[b' '; 13]);
        }
        self.status_items(9, request >> 4 & 7, b"OPF", 1);
        if request > 0x7F {
            self.set_latch(0x07);
            self.status_text(13, b"ATTRBS:    ");
            self.status_items(20, self.shadow >> 3 & 0x0F, b"RBFU", 1);
            self.status_text(24, &[b' '; 34]);
            self.status_text(25, b"MODE:");
            let column = self.status_items(30, self.mode, MODE_NAMES, 4);
            if self.mode & HDX == 0 {
                self.status_text(column, b"FDX ");
            }
            let locked = lock != 0 || self.kbd_locked & 1 != 0;
            self.status_text(58, if locked { b"LOCK" } else { b"    " });
            self.lock_shown = lock;
            self.status_text(
                63,
                if self.printing & 1 != 0 {
                    b"PTG"
                } else {
                    b"   "
                },
            );
            if self.transparent & 1 != 0 {
                self.status_text(63, b"TPR");
            }
            self.status_text(76, b"PG ");
            self.write_status(79, self.page.wrapping_add(b'1'));
        }
        if request & 0x0F != 0 {
            self.set_latch(0x27);
            // two errors before the line is written give the number that
            // their numbers ORed are: MODE ERR and CURS ERR read BAD ROM
            if let Some(message) = MESSAGES.get(usize::from(request & 0x0F) - 1) {
                self.status_text(67, *message);
            }
        }
        self.set_latch(self.shadow);
        self.status_req = 0;
    }

    // ------------------------------------------------------------ keys

    fn press(&mut self, code: u8, prog: Option<bool>) {
        let tables = [&KEYS_PLAIN, &KEYS_CTRL, &KEYS_SHIFT, &KEYS_CAPS];
        if self.hung || !tables.iter().any(|table| table.contains(&code)) {
            return;
        }
        // the scan takes no key while more than eight wait (0F73)
        if self.keyq_in.wrapping_sub(self.keyq_out) & 0x0F > 8 {
            return;
        }
        // a key during a print: no CR to the host at its end (0F9A)
        self.prn_ack = 0;
        self.key_accept(code, prog);
        self.settle();
    }

    /// A key the scan has found (0ED5).
    fn key_accept(&mut self, code: u8, prog: Option<bool>) {
        if self.switches.locked || self.kbd_locked & 1 != 0 {
            if code == 0x82 {
                self.reset_key();
            }
            return;
        }
        match code {
            0x80 => {}
            0x81 => {
                self.work += 1;
                self.caps = !self.caps;
                self.port0 = self.port0 & 0x7F | (self.caps & 0x80);
            }
            _ => match prog {
                Some(b) => self.fk_key(b, code),
                None => {
                    self.keyq_in = self.keyq_in.wrapping_add(1) & 0x0F;
                    self.keyq[usize::from(self.keyq_in)] = code;
                }
            },
        }
    }

    /// CTRL+CLEAR (07F7): the keyboard is unlocked, the message goes from
    /// the status line and the cursor comes back from it.
    fn reset_key(&mut self) {
        self.kbd_locked = 0;
        self.status_req = 0x81;
        self.cur_in_status = 0;
        self.prot_seek(0, 1);
    }

    /// The first key that waits (10C1) [D80 5.3].  A character is sent to
    /// the host unless in block mode, and shown in block mode and in half
    /// duplex; an escape sequence typed there is collected like one received.
    fn key_dispatch(&mut self) {
        self.key = self.keyq[usize::from(self.keyq_out.wrapping_add(1) & 0x0F)];
        if self.key >= 0xA0 {
            self.dispq_put_key();
        } else if self.key > 0x7F {
            self.local_key();
        } else {
            if self.mode & (BLK | HDX) != 0 {
                if self.key == 0x1B || self.esc_cnt != 0 {
                    self.rx_byte = self.key;
                    self.esc_collect();
                } else {
                    self.dispq_put_key();
                }
            }
            if self.mode & BLK == 0 {
                self.send(self.key);
                self.q_full = 0;
            }
        }
        if self.q_full & 1 == 0 {
            self.work += 1;
            self.keyq_out = self.keyq_out.wrapping_add(1) & 0x0F;
        }
    }

    /// A key into the queue of what was received (0FD3).  If the queue is
    /// full the key waits.
    fn dispq_put_key(&mut self) {
        self.q_full = 0xFF;
        if self.dispq_in.wrapping_add(1) != self.dispq_out {
            self.work += 1;
            self.dispq_in = self.dispq_in.wrapping_add(1);
            self.dispq[usize::from(self.dispq_in)] = self.key;
            self.q_full = 0;
        }
    }

    /// The keys 80h-9Fh (103E): carried out at once, never sent.
    fn local_key(&mut self) {
        self.q_full = 0;
        match self.key {
            // BREAK holds the line for a moment; nothing of that is modelled
            0x83 => return,
            0x82 => return self.reset_key(),
            0x8F => return self.test_pattern(),
            _ => {}
        }
        if self.key >= 0x90 && self.mode & PGM != 0 {
            return;
        }
        let Some(letter) = function_of_key(self.key) else {
            return;
        };
        self.key = letter | 0x80;
        self.escape(self.key);
        if self.mode & PGK != 0 {
            self.cur_byte = self.key;
            self.fk_capture();
        }
    }

    /// CTRL and the unmarked key: the pattern of the memory test over the
    /// page (2869).
    fn test_pattern(&mut self) {
        let mut value = 0u8;
        let mut pattern = 1u8;
        for row in 0x80..0x98u16 {
            self.set_latch(pattern | 7);
            for column in 0..COLUMNS as u16 {
                value = value & 0x7F | pattern & 0x80;
                self.write(row << 8 | column, value);
                value = value.wrapping_add(1);
            }
            pattern = pattern.rotate_left(1);
        }
        self.readback = false;
        self.set_latch(self.shadow);
    }

    // ------------------------------------------------------------ function keys

    fn fk_peek(&mut self, addr: u16) -> u8 {
        if (KEY_TABLE..KEY_MEMORY_END).contains(&addr) {
            self.fk_memory[usize::from(addr - KEY_TABLE)]
        } else {
            self.fk_lost = true;
            0xFF
        }
    }

    fn fk_poke(&mut self, addr: u16, value: u8) {
        if (KEY_TABLE..KEY_MEMORY_END).contains(&addr) {
            let byte = &mut self.fk_memory[usize::from(addr - KEY_TABLE)];
            self.work += u64::from(*byte != value);
            *byte = value;
        } else if addr != KEY_MEMORY_END {
            // one byte behind the memory is written when it is full, and
            // nothing is there
            self.fk_lost = true;
        }
    }

    fn fk_word(&mut self, addr: u16) -> u16 {
        u16::from(self.fk_peek(addr)) | u16::from(self.fk_peek(addr.wrapping_add(1))) << 8
    }

    fn fk_set_word(&mut self, addr: u16, value: u16) {
        self.fk_poke(addr, low(value));
        self.fk_poke(addr.wrapping_add(1), high(value));
    }

    /// Where the table has the start (`end` false) or the end of key `index`.
    fn fk_entry(index: u8, end: bool) -> u16 {
        KEY_TABLE + 4 * u16::from(index) + if end { 2 } else { 0 }
    }

    /// `ESC e k n` (23F4): key k (A or B) n (0 to 9) is programmed with what
    /// follows, up to `ESC x`.
    fn fk_program(&mut self) {
        if !self.switches.function_keys {
            self.dispq_get();
            self.dispq_get();
            return;
        }
        if self.mode & PGK != 0 {
            // the two parameters stay in the queue and are shown
            return;
        }
        let k = self.dispq_get().wrapping_sub(b'A');
        let mut index = low(u16::from(k) * 10);
        if index > 0x0A {
            index = 0x46;
        }
        let n = self.dispq_get();
        self.fk_index = index.wrapping_add(n).wrapping_sub(b'0');
        if self.fk_index >= 0x20 {
            self.status_req = PROG_ERR;
            return;
        }
        self.fk_delete(self.fk_index);
        self.fk_set_word(Screen::fk_entry(self.fk_index, false), self.fk_free);
        self.fk_ptr = self.fk_free;
        self.fk_recording = 0xFF;
        self.mode |= PGK;
        self.status_dirty();
    }

    /// Take a key's bytes out of the memory and close the gap (2324).  The
    /// bytes behind it are counted in eight bits: more than 255 of them are
    /// not all moved.
    fn fk_delete(&mut self, index: u8) {
        let start = self.fk_word(Screen::fk_entry(index, false));
        if start < KEY_STORE {
            return;
        }
        let end = self.fk_word(Screen::fk_entry(index, true));
        let behind = self.fk_free.wrapping_sub(end);
        if behind != 0 {
            let mut count = low(behind);
            let (mut from, mut to) = (end, start);
            loop {
                let byte = self.fk_peek(from);
                self.fk_poke(to, byte);
                from = from.wrapping_add(1);
                to = to.wrapping_add(1);
                count = count.wrapping_sub(1);
                if count == 0 {
                    break;
                }
            }
        }
        let length = end.wrapping_sub(start);
        let last = KEY_MEMORY_END - 1;
        if start > last {
            return;
        }
        self.fk_free = self.fk_free.wrapping_sub(length);
        for word in 0..40 {
            let addr = KEY_TABLE + 2 * word;
            let entry = self.fk_word(addr);
            if entry > start && entry < last {
                self.fk_set_word(addr, entry.wrapping_sub(length));
            }
        }
    }

    /// A byte for the key that is being programmed (2466).
    fn fk_capture(&mut self) {
        if self.fk_recording & 1 == 0 {
            return;
        }
        if self.cur_byte == 0xF8 {
            self.fk_recording = 0;
            self.mode &= !PGK;
            self.status_dirty();
            self.fk_set_word(Screen::fk_entry(self.fk_index, true), self.fk_ptr);
            self.fk_free = self.fk_ptr;
        } else {
            self.fk_poke(self.fk_ptr, self.cur_byte);
            if self.fk_ptr > KEY_MEMORY_END - 1 {
                self.status_req = PROG_ERR;
            } else {
                self.fk_ptr += 1;
            }
        }
    }

    /// PROG A or PROG B and a key (2569): play function key `code` - '0',
    /// ten more for PROG B.
    fn fk_key(&mut self, b: bool, code: u8) {
        if !self.switches.function_keys {
            return;
        }
        let mut index = code.wrapping_sub(b'0');
        if index > 0x14 {
            return;
        }
        if b {
            index += 10;
        }
        self.fk_to_host = 0;
        self.fk_index = index;
        self.mode |= PGK;
        self.fk_recording = 0;
        self.fk_ptr = self.fk_word(Screen::fk_entry(index, false));
        self.status_dirty();
    }

    /// A stretch of a function key being played (25BD): nine bytes, and on
    /// to the end of an escape sequence.  If the key cannot go on while an
    /// escape sequence is not complete, the firmware never leaves this
    /// routine: a key that is being sent to the host stops for good at ESC
    /// and a control code or a blank (252E), and that with a sequence begun
    /// before the key switched to the host, or before it was played, hangs
    /// the terminal.
    fn fk_pump(&mut self) {
        if self.mode & PGK == 0 || self.fk_recording & 1 != 0 {
            return;
        }
        loop {
            let before = (
                self.fk_ptr,
                self.fk_to_host,
                self.esc_cnt,
                self.dispq_in,
                self.status_req,
            );
            for _ in 0..9 {
                let end = self.fk_word(Screen::fk_entry(self.fk_index, true));
                if self.fk_ptr >= end {
                    self.mode &= !PGK;
                    self.status_dirty();
                    return;
                }
                if self.fk_to_host & 1 != 0 {
                    self.fk_play_host();
                } else {
                    self.fk_play_local();
                }
            }
            if self.esc_cnt == 0 {
                return;
            }
            if before
                == (
                    self.fk_ptr,
                    self.fk_to_host,
                    self.esc_cnt,
                    self.dispq_in,
                    self.status_req,
                )
            {
                self.hung = true;
                return;
            }
        }
    }

    /// A byte of a function key as if it had been received (24C4).  `ESC S`
    /// sends what follows to the host.
    fn fk_play_local(&mut self) {
        let byte = self.fk_peek(self.fk_ptr);
        self.rx_byte = byte;
        if byte == 0xD3 {
            self.fk_to_host = 0xFF;
        } else if byte < 0x80 && self.esc_cnt == 0 && byte != 0x1B {
            self.dispq_put();
        } else {
            if self.esc_cnt == 0 {
                self.esc_collect();
            }
            self.rx_byte = byte & 0x7F;
            self.esc_collect();
        }
        if self.status_req & 0x7F == 0 {
            self.work += 1;
            self.fk_ptr = self.fk_ptr.wrapping_add(1);
        }
    }

    /// A byte of a function key to the host (252E).  `ESC V` goes back to
    /// the screen.  An escape function is sent as ESC and the letter with
    /// bit 7; ESC and a control code, or ESC and a blank, are never got past.
    fn fk_play_host(&mut self) {
        let byte = self.fk_peek(self.fk_ptr);
        if byte == 0xD6 {
            self.work += 1;
            self.fk_to_host = 0;
            return;
        }
        if byte > 0xA0 {
            self.send(0x1B);
        } else if byte > 0x7F {
            return;
        }
        self.send(byte);
        self.fk_ptr = self.fk_ptr.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(s: &mut Screen, bytes: &[u8]) {
        for &b in bytes {
            s.put(b);
        }
    }

    #[test]
    fn the_sequences_the_station_sends() {
        let mut s = Screen::of(b"ONE\r\nTWO\r\nTHREE");
        assert_eq!(s.text(), "ONE\nTWO\nTHREE");
        assert_eq!(s.cursor(), (2, 5));
        // cursor to line 1, column 1; erase to the end of the line
        put(&mut s, b"\x1b=!!\x1bT");
        assert_eq!(s.text(), "ONE\nT\nTHREE");
        // cursor right twice, then a character
        put(&mut s, b"\x0c\x0cX");
        assert_eq!(s.line(1), "T  X");
        // delete line 1: the lines below move up, the cursor stays
        put(&mut s, b"\x1bR");
        assert_eq!(s.text(), "ONE\nTHREE");
        assert_eq!(s.cursor(), (1, 4));
        // clear: the cells are empty, not blanks
        put(&mut s, b"\x07\x1b*A");
        assert_eq!((s.text().as_str(), s.bells, s.cursor()), ("A", 1, (0, 1)));
        assert_eq!(s.line_cell(0, 0, 1).code, 0);
        // a line-drawing character
        put(&mut s, b"\x1bGA\x1bGI\x1bGJ\x1bGZ");
        assert_eq!(s.line(0), "A+-|");
        assert_eq!(s.line_cell(0, 0, 1).code, 1);
    }

    #[test]
    fn a_scrolling_console() {
        let mut text = Vec::new();
        for n in 0..30 {
            text.extend(format!("\n\rLINE {}", n).bytes());
        }
        let s = Screen::of(&text);
        assert_eq!(s.line(0), "LINE 6");
        assert_eq!(s.line(23), "LINE 29");
        // nothing was moved: the last line is row 6 of the memory
        assert_eq!(s.bottom(0), 6);
        assert_eq!(s.cursor_memory(), (6, 7));
        // the last column of the last line: the screen moves up at once
        let mut s = Screen::of(b"\x1b=7oZ");
        assert_eq!((s.cursor(), s.line(22).len()), ((23, 0), 80));
        // in block mode the cursor goes to the first line and nothing moves
        put(&mut s, b"\x1bB\x1b=7oY");
        assert_eq!((s.cursor(), s.bottom(0)), ((0, 0), 0));
    }

    #[test]
    fn the_cursor_address() {
        // a row off the screen: CURS ERR, and the cursor is in the status line
        let mut s = Screen::of(b"\x1b=8 ");
        assert_eq!(&s.status_line()[67..75], "CURS ERR");
        assert!(s.status_cell(67).1);
        assert!(s.cursor_in_status());
        assert_eq!(s.cursor(), (0, 0));
        put(&mut s, b"\x1b=\"#");
        assert!(!s.cursor_in_status());
        assert_eq!(s.cursor(), (2, 3));
        // a row byte below 20h is taken
        put(&mut s, b"\x1b=\x1f ");
        assert_eq!(s.cursor(), (23, 0));
        // the message stays until CTRL+CLEAR
        assert_eq!(&s.status_line()[67..75], "CURS ERR");
        s.key(0x82);
        assert_eq!(&s.status_line()[67..75], "        ");
        // the answer to ESC ?
        put(&mut s, b"\x1b='A\x1b?");
        assert_eq!(s.take_sent(), [0x27, 0x41]);
    }

    #[test]
    fn attributes_and_the_status_line() {
        let mut s = Screen::default();
        assert_eq!(
            s.status_line(),
            format!("{:13}ATTRBS:{:5}MODE:FDX {:42}PG 1", "", "", "")
        );
        // without write attribute mode the attribute is not stored
        put(&mut s, b"\x1bja");
        assert_eq!(
            s.line_cell(0, 0, 0),
            Cell {
                code: b'a',
                protect: false,
                attributes: 0
            }
        );
        put(&mut s, b"\x1bA\x1blb\x1b)c");
        assert_eq!(
            s.line_cell(0, 0, 1).attributes,
            Screen::REVERSE | Screen::UNDERLINE
        );
        assert!(s.line_cell(0, 0, 2).protect);
        // the letters and the names stand one behind the other
        assert_eq!(&s.status_line()[13..45], "ATTRBS:RU   MODE:ATB WPT FDX    ");
        assert_eq!(
            (s.mode(), s.attributes()),
            (ATB | WPT, Screen::REVERSE | Screen::UNDERLINE)
        );
        // 1Ah clears everything and switches the attributes off, not the modes
        s.put(0x1A);
        assert_eq!(
            s.line_cell(0, 0, 1),
            Cell {
                code: b' ',
                protect: false,
                attributes: 0
            }
        );
        assert_eq!(
            (s.mode(), s.attributes(), s.cursor()),
            (ATB | WPT, 0, (0, 0))
        );
    }

    #[test]
    fn tab_stops() {
        // none when switched on
        let mut s = Screen::of(b"\t");
        assert_eq!(s.cursor(), (0, 0));
        put(&mut s, b"\x1b=!*\x1b1\x1b=!k\x1b1\r\t");
        assert_eq!(s.cursor(), (1, 10));
        assert!(s.tab_stop(10) && s.tab_stop(75));
        s.put(b'\t');
        assert_eq!(s.cursor(), (1, 75));
        // any erase takes the stops of columns 72 to 79 away
        put(&mut s, b"\x1bT\r\t\t");
        assert_eq!(s.cursor(), (1, 10));
        assert!(!s.tab_stop(75));
    }

    #[test]
    fn protect_mode_and_sending() {
        let mut s = Screen::of(b"\x1b)Name:\x1b(     \x1b)Age:\x1b(\x1b=  \x1b&");
        assert_eq!(s.cursor(), (0, 5));
        put(&mut s, b"Ann\t");
        assert_eq!(s.cursor(), (0, 14));
        put(&mut s, b"7\x1b5");
        assert_eq!(s.take_sent(), b"\x1cAnn  \x1c7\r");
        put(&mut s, b"\x1b7");
        assert_eq!(s.take_sent(), b"\x1b)Name:\x1b(Ann  \x1b)Age:\x1b(7\r");
        // erasing keeps the protected cells
        put(&mut s, b"\x1b;");
        assert_eq!(s.line(0), "Name:     Age:");
        // insert line does nothing in protect mode
        put(&mut s, b"\x1bE");
        assert_eq!(s.line(0), "Name:     Age:");
        // nothing left to stand on: PROT ERR
        put(&mut s, b"\x1b)\x1bf  7o");
        assert_eq!(&s.status_line()[67..75], "PROT ERR");
    }

    #[test]
    fn keys() {
        let mut s = Screen::default();
        let a = Screen::key_code(2, 2, false, false, false);
        assert_eq!(
            (a, Screen::key_code(2, 2, true, false, false)),
            (b'a', b'A')
        );
        s.key(a);
        assert_eq!((s.take_sent(), s.text()), (vec![b'a'], String::new()));
        // block mode: to the screen, not to the host
        s.key(Screen::key_code(12, 2, true, false, false));
        assert_eq!(s.mode(), BLK);
        s.key(a);
        assert_eq!((s.take_sent(), s.text()), (vec![], "a".to_string()));
        // LINE ERASE, a function of the terminal's own
        s.key(0x08);
        s.key(Screen::key_code(6, 6, false, false, false));
        assert_eq!(s.line_cell(0, 0, 0).code, b' ');
        // locked: only CTRL+CLEAR does anything
        put(&mut s, b"\x1b#");
        s.key(a);
        assert!(s.keyboard_locked());
        assert_eq!((s.cursor(), &s.status_line()[58..62]), ((0, 0), "LOCK"));
        s.key(Screen::key_code(11, 4, false, true, false));
        assert!(!s.keyboard_locked());
        // half duplex: both
        let mut s = Screen::with(|w| w.half_duplex = true);
        s.key(a);
        assert_eq!((s.take_sent(), s.text()), (vec![b'a'], "a".to_string()));
    }

    #[test]
    fn program_mode() {
        let mut s = Screen::of(b"\x1bc\r\x1bj");
        let codes: Vec<u8> = (0..4).map(|c| s.line_cell(0, 0, c).code).collect();
        assert_eq!(codes, [0x0D, 0x1B, b'j', 0]);
        assert_eq!(s.line(0), "??j");
        put(&mut s, b"\x1bd\r");
        assert_eq!((s.mode(), s.cursor()), (0, (0, 0)));
    }

    #[test]
    fn pages() {
        let mut s = Screen::of(b"one\x1bNtwo");
        assert_eq!((s.page(), s.text().as_str()), (1, "   two"));
        assert_eq!(&s.status_line()[76..], "PG 2");
        put(&mut s, b"\x1bN");
        assert_eq!((s.page(), s.text().as_str()), (0, "one"));
        // auto flip: off the last line is onto the next page
        put(&mut s, b"\x1bv\x1b=7 \n");
        assert_eq!((s.page(), s.cursor()), (1, (0, 0)));
        let mut s = Screen::with(|w| w.pages = 4);
        put(&mut s, b"\x1bN\x1bN\x1bN");
        assert_eq!((s.pages(), s.page()), (4, 3));
    }

    #[test]
    fn no_printer() {
        // a print of one cell comes to its end: CR to the host, PTG for good
        let mut s = Screen::of(b"\x1bP");
        assert_eq!((s.take_sent(), s.printing()), (vec![0x0D], true));
        assert_eq!(&s.status_line()[63..66], "PTG");
        // ESC J: three bytes are taken, the rest waits and is never shown
        let s = Screen::of(b"\x1bJabcdef\x1bK");
        assert!(s.transparent());
        assert_eq!((s.text(), &s.status_line()[63..66]), (String::new(), "TPR"));
    }

    #[test]
    fn a_terminal_that_hangs() {
        // every cell of the last page protected, protect mode and auto flip:
        // clearing all pages never ends
        let mut s = Screen::of(b"\x1bN\x1b)\x1bf  7o\x1b(\x1bF\x1bv\x1b&");
        assert!(!s.hung());
        put(&mut s, b"\x1bXabc");
        assert!(s.hung());
        assert_eq!(s.line(0), "");
    }

    #[test]
    fn function_keys() {
        let mut s = Screen::with(|w| w.function_keys = true);
        put(&mut s, b"\x1beA1hello\x1bSthere\x1bx\x1a");
        assert_eq!(s.mode(), 0);
        s.prog_key(false, b'1');
        assert_eq!(
            (s.text().as_str(), s.take_sent()),
            ("hello", b"there".to_vec())
        );
        // the key's number is its code less 30h: with caps lock, A is key 17.
        // What a key is programmed with is shown as it comes.
        put(&mut s, b"\x1beB7x\x1bx");
        assert_eq!(s.text(), "hellox");
        s.prog_key(false, b'A');
        assert_eq!(s.text(), "helloxx");
        // without them the sequence and its parameters are taken, no more
        let s = Screen::of(b"\x1beA1x");
        assert_eq!((s.text().as_str(), s.mode()), ("x", 0));
    }
}
