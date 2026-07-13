//! Emulator state snapshot for rewind functionality.
//!
//! Captures the minimum state needed to restore the emulator to a previous
//! point in time. Snapshots are stored in a ring buffer, taken every N frames
//! (default 60 = 1 second), allowing rewind of up to `capacity` seconds.
//!
//! ## Usage
//!
//! ```text
//! // GUI: hold Backspace to rewind
//! // Step mode: `rewind` command
//! ```

/// A frozen snapshot of emulator state.
#[derive(Clone)]
pub struct Snapshot {
    /// CPU: pc, sp, sreg, tick, sleeping
    pub pc: u16,
    pub sp: u16,
    pub sreg: u8,
    pub tick: u64,
    pub sleeping: bool,
    /// Full data-space (registers + I/O + SRAM)
    pub data: Vec<u8>,
    /// EEPROM contents
    pub eeprom: Vec<u8>,
    /// Display framebuffer (SSD1306 or PCD8544)
    pub framebuffer: Vec<u8>,
    /// Frame number when this snapshot was taken
    pub frame: u32,
}

/// Ring buffer of snapshots for rewind.
pub struct RewindBuffer {
    buf: Vec<Option<Snapshot>>,
    /// Write position (next slot to overwrite)
    write_pos: usize,
    /// Number of valid snapshots
    count: usize,
    /// Frames between snapshots
    pub interval: u32,
    /// Frame counter for interval tracking
    frame_counter: u32,
}

impl RewindBuffer {
    /// Create a new rewind buffer with given capacity (number of snapshots).
    ///
    /// With interval=60 and capacity=300, stores 5 minutes of rewind at 60fps.
    pub fn new(capacity: usize, interval: u32) -> Self {
        let mut buf = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            buf.push(None);
        }
        RewindBuffer {
            buf,
            write_pos: 0,
            count: 0,
            interval,
            frame_counter: 0,
        }
    }

    /// Notify that a frame has completed. Returns true if a snapshot should be taken.
    pub fn tick_frame(&mut self) -> bool {
        self.frame_counter += 1;
        if self.frame_counter >= self.interval {
            self.frame_counter = 0;
            true
        } else {
            false
        }
    }

    /// Push a snapshot into the ring buffer.
    pub fn push(&mut self, snap: Snapshot) {
        self.buf[self.write_pos] = Some(snap);
        self.write_pos = (self.write_pos + 1) % self.buf.len();
        if self.count < self.buf.len() {
            self.count += 1;
        }
    }

    /// Pop the most recent snapshot (for rewind). Returns None if empty.
    pub fn pop(&mut self) -> Option<Snapshot> {
        if self.count == 0 {
            return None;
        }
        // Move write_pos back
        if self.write_pos == 0 {
            self.write_pos = self.buf.len() - 1;
        } else {
            self.write_pos -= 1;
        }
        self.count -= 1;
        self.buf[self.write_pos].take()
    }

    /// Number of stored snapshots.
    pub fn len(&self) -> usize {
        self.count
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Clear all snapshots.
    pub fn clear(&mut self) {
        for slot in self.buf.iter_mut() {
            *slot = None;
        }
        self.count = 0;
        self.write_pos = 0;
        self.frame_counter = 0;
    }

    /// Estimated memory usage in bytes.
    pub fn memory_usage(&self) -> usize {
        self.count * (std::mem::size_of::<Snapshot>() + 3072 + 1024 + 8192)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_snap(frame: u32) -> Snapshot {
        Snapshot {
            pc: 0,
            sp: 0,
            sreg: 0,
            tick: 0,
            sleeping: false,
            data: vec![0; 32],
            eeprom: vec![0; 16],
            framebuffer: vec![0; 64],
            frame,
        }
    }

    #[test]
    fn test_push_pop() {
        let mut rb = RewindBuffer::new(3, 1);
        rb.push(make_snap(1));
        rb.push(make_snap(2));
        rb.push(make_snap(3));
        assert_eq!(rb.len(), 3);

        let s = rb.pop().unwrap();
        assert_eq!(s.frame, 3);
        let s = rb.pop().unwrap();
        assert_eq!(s.frame, 2);
        assert_eq!(rb.len(), 1);
    }

    #[test]
    fn test_ring_overflow() {
        let mut rb = RewindBuffer::new(2, 1);
        rb.push(make_snap(1));
        rb.push(make_snap(2));
        rb.push(make_snap(3)); // overwrites frame 1
        assert_eq!(rb.len(), 2);

        let s = rb.pop().unwrap();
        assert_eq!(s.frame, 3);
        let s = rb.pop().unwrap();
        assert_eq!(s.frame, 2);
        assert!(rb.pop().is_none());
    }

    #[test]
    fn test_tick_frame() {
        let mut rb = RewindBuffer::new(10, 60);
        for _ in 0..59 {
            assert!(!rb.tick_frame());
        }
        assert!(rb.tick_frame()); // 60th frame
    }
}
