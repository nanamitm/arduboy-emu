//! Execution profiler for AVR programs.
//!
//! Tracks instruction-level execution statistics:
//! - Per-address hit counts (PC histogram)
//! - Total instruction and cycle counts
//! - Top-N hotspot analysis with disassembly
//! - Call graph tracking (CALL/RET pairs)
//!
//! The profiler is zero-cost when disabled — all data lives in this struct,
//! and the emulator core calls [`Profiler::record`] only when enabled.

use std::collections::HashMap;

/// Execution profiler state.
pub struct Profiler {
    /// Whether profiling is currently active
    pub enabled: bool,
    /// Per-PC hit counts (word address → count)
    pc_hits: HashMap<u16, u64>,
    /// Total instructions executed while profiling
    pub total_instructions: u64,
    /// Total cycles elapsed while profiling
    pub total_cycles: u64,
    /// Cycle counter at profiler start
    start_tick: u64,
    /// Call stack depth tracker: (caller_pc, callee_pc) → count
    call_graph: HashMap<(u16, u16), u64>,
    /// Current call stack for tracking (limited depth)
    call_stack: Vec<u16>,
}

impl Profiler {
    pub fn new() -> Self {
        Profiler {
            enabled: false,
            pc_hits: HashMap::new(),
            total_instructions: 0,
            total_cycles: 0,
            start_tick: 0,
            call_graph: HashMap::new(),
            call_stack: Vec::new(),
        }
    }

    /// Start or restart profiling, clearing all accumulated data.
    pub fn start(&mut self, tick: u64) {
        self.pc_hits.clear();
        self.call_graph.clear();
        self.call_stack.clear();
        self.total_instructions = 0;
        self.total_cycles = 0;
        self.start_tick = tick;
        self.enabled = true;
    }

    /// Stop profiling, finalize cycle count.
    pub fn stop(&mut self, tick: u64) {
        self.total_cycles = tick.saturating_sub(self.start_tick);
        self.enabled = false;
    }

    /// Record execution of an instruction at the given PC (word address).
    #[inline]
    pub fn record(&mut self, pc: u16) {
        *self.pc_hits.entry(pc).or_insert(0) += 1;
        self.total_instructions += 1;
    }

    /// Record a CALL/RCALL/ICALL instruction.
    #[inline]
    pub fn record_call(&mut self, caller_pc: u16, target_pc: u16) {
        *self.call_graph.entry((caller_pc, target_pc)).or_insert(0) += 1;
        if self.call_stack.len() < 128 {
            self.call_stack.push(caller_pc);
        }
    }

    /// Record a RET/RETI instruction.
    #[inline]
    pub fn record_ret(&mut self) {
        self.call_stack.pop();
    }

    /// Get number of unique addresses executed.
    pub fn unique_addresses(&self) -> usize {
        self.pc_hits.len()
    }

    /// Get top-N hottest addresses by execution count.
    pub fn top_hits(&self, n: usize) -> Vec<(u16, u64)> {
        let mut v: Vec<_> = self.pc_hits.iter().map(|(&pc, &cnt)| (pc, cnt)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        v.truncate(n);
        v
    }

    /// Get top-N call edges by invocation count.
    pub fn top_calls(&self, n: usize) -> Vec<((u16, u16), u64)> {
        let mut v: Vec<_> = self
            .call_graph
            .iter()
            .map(|(&edge, &cnt)| (edge, cnt))
            .collect();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        v.truncate(n);
        v
    }

    /// Get flat profile: addresses grouped into ranges (basic blocks).
    /// Returns sorted vec of (start_addr, end_addr, total_hits).
    pub fn flat_profile(&self) -> Vec<(u16, u16, u64)> {
        if self.pc_hits.is_empty() {
            return vec![];
        }
        let mut addrs: Vec<_> = self.pc_hits.keys().copied().collect();
        addrs.sort();

        let mut ranges = Vec::new();
        let mut start = addrs[0];
        let mut end = start;
        let mut hits = *self.pc_hits.get(&start).unwrap_or(&0);

        for &addr in &addrs[1..] {
            if addr <= end + 2 {
                // Contiguous (allow gap of 1 for 2-word instructions)
                end = addr;
                hits += self.pc_hits.get(&addr).unwrap_or(&0);
            } else {
                ranges.push((start, end, hits));
                start = addr;
                end = addr;
                hits = *self.pc_hits.get(&addr).unwrap_or(&0);
            }
        }
        ranges.push((start, end, hits));
        ranges.sort_by(|a, b| b.2.cmp(&a.2));
        ranges
    }

    /// Format a full profiling report.
    pub fn report(&self, flash: &[u8]) -> String {
        let mut s = String::new();
        s.push_str(&format!("=== Profiler Report ===\n"));
        s.push_str(&format!("Instructions: {}\n", self.total_instructions));
        s.push_str(&format!("Cycles: {}\n", self.total_cycles));
        s.push_str(&format!("Unique addresses: {}\n", self.unique_addresses()));
        if self.total_instructions > 0 {
            let cpi = self.total_cycles as f64 / self.total_instructions as f64;
            s.push_str(&format!("Cycles/instruction: {:.2}\n", cpi));
        }

        s.push_str(&format!("\n--- Top 20 Hotspots ---\n"));
        s.push_str(&format!(
            "{:>8}  {:>6}  {:>7}  {}\n",
            "Addr", "Hits", "%", "Instruction"
        ));
        for (pc, cnt) in self.top_hits(20) {
            let pct = if self.total_instructions > 0 {
                cnt as f64 / self.total_instructions as f64 * 100.0
            } else {
                0.0
            };
            let byte_addr = (pc as usize) * 2;
            let opcode = if byte_addr + 1 < flash.len() {
                (flash[byte_addr] as u16) | ((flash[byte_addr + 1] as u16) << 8)
            } else {
                0
            };
            let next = if byte_addr + 3 < flash.len() {
                (flash[byte_addr + 2] as u16) | ((flash[byte_addr + 3] as u16) << 8)
            } else {
                0
            };
            let (inst, _) = crate::opcodes::decode(opcode, next);
            let asm = crate::disasm::disassemble(inst, pc);
            s.push_str(&format!(
                "0x{:04X}  {:>6}  {:>6.2}%  {}\n",
                pc * 2,
                cnt,
                pct,
                asm
            ));
        }

        let calls = self.top_calls(10);
        if !calls.is_empty() {
            s.push_str(&format!("\n--- Top 10 Call Edges ---\n"));
            s.push_str(&format!(
                "{:>8} → {:>8}  {:>6}\n",
                "Caller", "Callee", "Count"
            ));
            for ((from, to), cnt) in calls {
                s.push_str(&format!(
                    "0x{:04X} → 0x{:04X}  {:>6}\n",
                    from * 2,
                    to * 2,
                    cnt
                ));
            }
        }

        let blocks = self.flat_profile();
        if !blocks.is_empty() {
            s.push_str(&format!("\n--- Top 10 Hot Regions ---\n"));
            for (start, end, hits) in blocks.iter().take(10) {
                let pct = if self.total_instructions > 0 {
                    *hits as f64 / self.total_instructions as f64 * 100.0
                } else {
                    0.0
                };
                s.push_str(&format!(
                    "0x{:04X}–0x{:04X}  {:>6} hits  ({:.1}%)\n",
                    start * 2,
                    end * 2,
                    hits,
                    pct
                ));
            }
        }

        s
    }
}

impl Default for Profiler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profiler_basic() {
        let mut p = Profiler::new();
        p.start(0);
        p.record(0x100);
        p.record(0x100);
        p.record(0x101);
        p.record(0x100);
        assert_eq!(p.total_instructions, 4);
        assert_eq!(p.unique_addresses(), 2);
        let top = p.top_hits(1);
        assert_eq!(top[0], (0x100, 3));
    }

    #[test]
    fn test_call_graph() {
        let mut p = Profiler::new();
        p.start(0);
        p.record_call(0x10, 0x200);
        p.record_call(0x10, 0x200);
        p.record_call(0x20, 0x300);
        let calls = p.top_calls(2);
        assert_eq!(calls[0], ((0x10, 0x200), 2));
    }
}
