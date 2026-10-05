//! What an observer sees of a run.

/// One observable action of the machine.  Register values are `None` when
/// undefined.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// An instruction issues.  `p` is its parcel address relative to BA.
    Issue {
        p: u32,
        parcel0: u16,
        parcel1: Option<u16>,
    },
    /// Ai is written.
    A { i: u8, value: Option<u32> },
    /// Si is written.
    S { i: u8, value: Option<u64> },
    /// Bjk is written.
    B { jk: u8, value: Option<u32> },
    /// Tjk is written.
    T { jk: u8, value: Option<u64> },
    /// Element `elem` of Vi is written.  A vector instruction reports its
    /// elements in element order.
    V { i: u8, elem: u8, value: Option<u64> },
    /// VL is written (the 7-bit register value).
    Vl(Option<u8>),
    /// VM is written.
    Vm(Option<u64>),
    /// P is loaded by an exchange.
    P(u32),
    /// BA is loaded by an exchange.
    Ba(u32),
    /// LA is loaded by an exchange.
    La(u32),
    /// XA is written (0013 or an exchange).
    Xa(u8),
    /// M is written (0021, 0022 or an exchange).
    Mode(u8),
    /// F is written (a flag sets, or an exchange).
    Flags(u16),
    /// The real-time clock is entered by 0014.
    Rtc(Option<u64>),
    /// A memory word is stored; `addr` is the absolute word address.
    Mem { addr: u32, value: Option<u64> },
    /// An exchange sequence begins with the package at `xa` * 16.  The
    /// sixteen stores and the register loads that follow belong to it.
    ExchangeStart { xa: u8 },
    /// The exchange sequence is complete.
    ExchangeEnd,
    /// A character is written to the console.
    Console(u8),
    /// TEST_EXIT is written.
    Exit(u64),
}

/// Receives the events of a run, in order.
pub trait Observer {
    fn event(&mut self, event: &Event);
}

impl<F: FnMut(&Event)> Observer for F {
    fn event(&mut self, event: &Event) {
        self(event)
    }
}
