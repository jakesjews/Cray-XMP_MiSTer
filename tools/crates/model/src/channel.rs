//! The 6 Mbyte channels of the X-MP mode: channels 10 to 17 octal, even
//! numbers input, odd numbers output (CSM-0111000 pages 2-15 to 2-19, 5-9,
//! 5-37 and appendix B).
//!
//! A channel moves 16-bit parcels between a device and central memory, four
//! to a word, parcel 0 (bits 63 to 48) first.  The program sets the limit
//! address CL (0011) and then the current address CA (0010), which makes the
//! channel active.  Channel addresses are absolute.
//!
//! * Input: the device sends a parcel with Ready and the channel answers
//!   Resume.  Every fourth parcel the word goes to memory at (CA) and CA
//!   advances; at (CA) = (CL) the channel sets its interrupt flag and goes
//!   inactive.  A Disconnect from the device does the same at once, storing a
//!   partly assembled word with zeros in the parcels that did not come.  A
//!   Ready that finds the channel inactive is held until it is activated.
//! * Output: the channel reads the word at (CA), advances CA and sends the
//!   four parcels, each with Ready, waiting for the device's Resume after
//!   each.  After the last parcel of the word that made (CA) = (CL) it sends
//!   Disconnect, sets its interrupt flag and goes inactive.
//! * 0012 clears the interrupt and error flags and stops the channel.  With
//!   k = 1 it also raises the Master Clear line of an output channel (k = 0
//!   drops it) and forgets a held Ready of an input channel.
//! * 033 reads the lowest numbered channel whose interrupt flag is set, a
//!   current address or an error flag.
//!
//! The device end is driven through the `channel_*` methods of `Machine`.
//! Nothing here raises the error flag: there is no parity and no way to
//! send a Resume or a Ready out of turn.

use crate::machine::Machine;
use cray1_isa::Cpu;

/// Mask of a channel address: four million words.
const ADDRESS: u32 = (1 << 22) - 1;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Channel {
    pub ca: u32,
    pub cl: u32,
    pub active: bool,
    pub interrupt: bool,
    pub error: bool,
    /// The assembly or disassembly register and the parcels in it.
    word: u64,
    count: u8,
    /// Input: a parcel that arrived while the channel was inactive.
    held: Option<u16>,
    /// Input: a Resume has been sent that the device has not yet seen.
    resumed: bool,
    /// Output: the Master Clear line to the device.
    pub master_clear: bool,
}

/// What a program can see of a channel, for tests and displays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChannelState {
    pub ca: u32,
    pub cl: u32,
    pub active: bool,
    pub interrupt: bool,
    pub error: bool,
    pub master_clear: bool,
}

impl Machine {
    /// The index of channel `number` (10 to 17 octal); the X-MP looks only
    /// at the low four bits of the number and passes over 0 to 7.
    pub(crate) fn channel_index(&self, number: u32) -> Option<usize> {
        (self.cpu == Cpu::Xmp && number & 0o17 >= 0o10).then_some((number & 7) as usize)
    }

    /// True if any channel asks for the I/O interrupt.
    pub(crate) fn channel_interrupt(&self) -> bool {
        self.channels.iter().any(|c| c.interrupt)
    }

    /// The lowest numbered channel with its interrupt flag set, for 033.
    pub(crate) fn channel_interrupting(&self) -> u32 {
        self.channels
            .iter()
            .position(|c| c.interrupt)
            .map_or(0, |n| 0o10 + n as u32)
    }

    /// 0010: enter CA and activate the channel.
    pub(crate) fn channel_activate(&mut self, n: usize, address: u32) {
        let c = &mut self.channels[n];
        c.ca = address & ADDRESS;
        c.active = true;
        c.count = 0;
        c.word = 0;
        if n % 2 == 1 {
            self.loop_waiting[n / 2] = false;
        } else {
            // a Ready that was held is answered now
            if let Some(parcel) = self.channels[n].held.take() {
                self.channel_accept(n, parcel);
            }
        }
    }

    /// 0012: clear the flags and stop the channel.
    pub(crate) fn channel_clear(&mut self, n: usize, k1: bool) {
        let c = &mut self.channels[n];
        c.interrupt = false;
        c.error = false;
        c.active = false;
        c.count = 0;
        if n % 2 == 1 {
            c.master_clear = k1;
            self.loop_waiting[n / 2] = false;
        } else if k1 {
            c.held = None;
        }
    }

    fn channel_store(&mut self, address: u32, word: u64) {
        if address < self.io_page() {
            self.store(address, Some(word));
        }
    }

    /// An input channel takes a parcel and answers Resume.
    fn channel_accept(&mut self, n: usize, parcel: u16) {
        let c = &mut self.channels[n];
        c.word = c.word << 16 | parcel as u64;
        c.count += 1;
        c.resumed = true;
        if c.count == 4 {
            let (address, word) = (c.ca, c.word);
            c.ca = (c.ca + 1) & ADDRESS;
            c.count = 0;
            c.word = 0;
            if c.ca == c.cl {
                c.interrupt = true;
                c.active = false;
            }
            self.channel_store(address, word);
        }
    }

    // ---- the device end

    /// What a program can see of channel `number` (10 to 17 octal).
    pub fn channel(&self, number: u32) -> ChannelState {
        let c = &self.channels[(number & 7) as usize];
        ChannelState {
            ca: c.ca,
            cl: c.cl,
            active: c.active,
            interrupt: c.interrupt,
            error: c.error,
            master_clear: c.master_clear,
        }
    }

    /// The device of input channel `number` sends a parcel with Ready.
    /// Returns true if the channel answered Resume at once; if not, the
    /// Ready is held and `channel_resumed` tells when the answer comes.
    pub fn channel_input(&mut self, number: u32, parcel: u16) -> bool {
        let n = (number & 6) as usize;
        if self.channels[n].active {
            self.channel_accept(n, parcel);
            self.channels[n].resumed = false;
            true
        } else {
            self.channels[n].held = Some(parcel);
            false
        }
    }

    /// True once after a held Ready of input channel `number` was answered.
    pub fn channel_resumed(&mut self, number: u32) -> bool {
        std::mem::take(&mut self.channels[(number & 6) as usize].resumed)
    }

    /// The device of input channel `number` sends Disconnect: the end of
    /// its data.  Ignored unless the channel is active.
    pub fn channel_disconnect(&mut self, number: u32) {
        let n = (number & 6) as usize;
        let c = &mut self.channels[n];
        if !c.active {
            return;
        }
        c.interrupt = true;
        c.active = false;
        if c.count != 0 {
            // the parcels that did not come are zero
            let (address, word) = (c.ca, c.word << (16 * (4 - c.count as u32)));
            c.count = 0;
            c.word = 0;
            self.channel_store(address, word);
        }
    }

    /// The parcel output channel `number` holds out with Ready, if any.
    pub fn channel_output(&mut self, number: u32) -> Option<u16> {
        let n = (number & 6) as usize + 1;
        if !self.channels[n].active {
            return None;
        }
        if self.channels[n].count == 0 {
            // read the next word and advance the current address
            let address = self.channels[n].ca;
            let word = if address < self.io_page() {
                self.mem.get(address).unwrap_or(0)
            } else {
                0
            };
            let c = &mut self.channels[n];
            c.word = word;
            c.count = 4;
            c.ca = (c.ca + 1) & ADDRESS;
        }
        let c = &self.channels[n];
        Some((c.word >> (16 * (c.count as u32 - 1))) as u16)
    }

    /// The device answers the parcel of `channel_output` with Resume.
    /// Returns true if the channel then sends Disconnect: that parcel was
    /// the last of the transfer.
    pub fn channel_resume(&mut self, number: u32) -> bool {
        let n = (number & 6) as usize + 1;
        let c = &mut self.channels[n];
        if !c.active || c.count == 0 {
            return false;
        }
        c.count -= 1;
        if c.count == 0 && c.ca == c.cl {
            c.interrupt = true;
            c.active = false;
            return true;
        }
        false
    }

    /// Test fixture: connect each output channel to the input channel of
    /// its pair, as a cable from 11 to 10 would.
    pub fn set_channel_loopback(&mut self, on: bool) {
        self.channel_loopback = on;
    }

    /// Move what the looped-back channels can move now.
    pub(crate) fn channel_loop(&mut self) {
        for pair in 0..4u32 {
            let (input, output) = (0o10 + 2 * pair, 0o11 + 2 * pair);
            // a held Ready that has since been answered
            if self.loop_waiting[pair as usize] {
                if !self.channel_resumed(input) {
                    continue;
                }
                self.loop_waiting[pair as usize] = false;
                if self.channel_resume(output) {
                    self.channel_disconnect(input);
                    continue;
                }
            }
            while let Some(parcel) = self.channel_output(output) {
                if !self.channel_input(input, parcel) {
                    self.loop_waiting[pair as usize] = true;
                    break;
                }
                if self.channel_resume(output) {
                    self.channel_disconnect(input);
                    break;
                }
            }
        }
    }
}
