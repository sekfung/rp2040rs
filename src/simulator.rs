use std::cell::Cell;
use std::rc::Rc;

use crate::clock::SimulationClock;
use crate::gdb::target::GdbTarget;
use crate::rp2040::Rp2040;

pub struct Simulator {
    pub rp2040: Rp2040,
    clock: Rc<SimulationClock>,
    stopped: bool,
    break_flag: Rc<Cell<bool>>,
}

impl Simulator {
    pub fn new() -> Self {
        let clock = Rc::new(SimulationClock::new());
        let rp2040 = Rp2040::with_clock(clock.clone());
        let break_flag = Rc::new(Cell::new(false));
        let flag = break_flag.clone();
        rp2040.set_on_break(Box::new(move |_| flag.set(true)));
        Self {
            rp2040,
            clock,
            stopped: true,
            break_flag,
        }
    }

    pub fn execute_once(&mut self) {
        self.execute_steps(1_000_000);
    }

    /// Execute at most `limit` instructions and leave the target stopped.
    pub fn execute_steps(&mut self, limit: usize) {
        self.stopped = false;
        self.break_flag.set(false);
        let cycle_nanos = 1e9 / 125_000_000.0;
        for _ in 0..limit {
            if self.stopped || self.break_flag.get() {
                break;
            }
            if self.rp2040.core.waiting() {
                let nanos = self.clock.nanos_to_next_alarm();
                if nanos <= 0.0 {
                    break;
                }
                self.clock.tick(nanos);
            } else {
                let cycles = self.rp2040.execute_instruction();
                self.clock.tick(cycles as f64 * cycle_nanos);
            }
        }
        // A batch is synchronous: report stopped until the next continue.
        self.stopped = true;
    }
}

impl Default for Simulator {
    fn default() -> Self {
        Self::new()
    }
}

impl GdbTarget for Simulator {
    fn rp2040(&self) -> &Rp2040 {
        &self.rp2040
    }

    fn executing(&self) -> bool {
        !self.stopped
    }

    fn execute(&mut self) {
        self.execute_once();
    }

    fn step(&mut self) {
        self.break_flag.set(false);
        self.rp2040.step();
    }

    fn stop(&mut self) {
        self.stopped = true;
    }

    fn last_break(&self) -> bool {
        self.break_flag.get()
    }
}
