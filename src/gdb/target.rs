use crate::rp2040::Rp2040;

pub trait GdbTarget {
    fn rp2040(&self) -> &Rp2040;
    fn executing(&self) -> bool;
    fn execute(&mut self);
    fn stop(&mut self);
    fn last_break(&self) -> bool {
        false
    }
}
