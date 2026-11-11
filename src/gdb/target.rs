use crate::rp2040::Rp2040;

pub trait GdbTarget {
    fn rp2040(&self) -> &Rp2040;
    fn executing(&self) -> bool;
    fn execute(&mut self);
    /// Execute exactly one instruction. Implementations must reset their
    /// break flag before stepping so `last_break` reports this step only.
    fn step(&mut self) {
        self.rp2040().step();
    }
    fn stop(&mut self);
    /// Set by the target when BKPT fires. Implementations that can signal
    /// breaks must override this; the default is "no break observed".
    fn last_break(&self) -> bool {
        false
    }
}
