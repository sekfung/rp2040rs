#[derive(Debug, Clone)]
pub struct Fifo {
    buffer: Vec<u32>,
    start: usize,
    used: usize,
}

impl Fifo {
    pub fn new(size: usize) -> Self {
        Self {
            buffer: vec![0; size],
            start: 0,
            used: 0,
        }
    }

    pub fn size(&self) -> usize {
        self.buffer.len()
    }

    pub fn item_count(&self) -> usize {
        self.used
    }

    pub fn push(&mut self, value: u32) {
        let length = self.buffer.len();
        if self.used < length {
            let end = self.start + self.used;
            let index = if end >= length { end - length } else { end };
            self.buffer[index] = value;
            self.used += 1;
        }
    }

    pub fn pull(&mut self) -> u32 {
        if self.used == 0 {
            return 0;
        }
        let value = self.buffer[self.start];
        self.start += 1;
        if self.start == self.buffer.len() {
            self.start = 0;
        }
        self.used -= 1;
        value
    }

    pub fn peek(&self) -> u32 {
        if self.used == 0 {
            0
        } else {
            self.buffer[self.start]
        }
    }

    pub fn reset(&mut self) {
        self.used = 0;
    }

    pub fn empty(&self) -> bool {
        self.used == 0
    }

    pub fn full(&self) -> bool {
        self.used == self.buffer.len()
    }

    pub fn items(&self) -> Vec<u32> {
        (0..self.used)
            .map(|i| self.buffer[(self.start + i) % self.buffer.len()])
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::Fifo;

    #[test]
    fn should_successfully_push_and_pull_4_items() {
        let mut fifo = Fifo::new(3);
        assert!(fifo.empty());
        fifo.push(1);
        assert!(!fifo.empty());
        fifo.push(2);
        assert_eq!(fifo.item_count(), 2);
        assert!(!fifo.full());
        fifo.push(3);
        assert!(fifo.full());
        assert_eq!(fifo.pull(), 1);
        assert!(!fifo.full());
        fifo.push(4);
        assert!(fifo.full());
        assert_eq!(fifo.pull(), 2);
        assert_eq!(fifo.pull(), 3);
        assert!(!fifo.empty());
        assert_eq!(fifo.item_count(), 1);
        assert_eq!(fifo.pull(), 4);
        assert!(!fifo.full());
        assert!(fifo.empty());
    }

    #[test]
    fn peek_should_not_change_fifo_contents() {
        let mut fifo = Fifo::new(3);
        assert!(fifo.empty());
        fifo.push(10);
        assert!(!fifo.empty());
        fifo.push(20);
        assert_eq!(fifo.peek(), 10);
        assert_eq!(fifo.item_count(), 2);
        assert_eq!(fifo.pull(), 10);
    }

    #[test]
    fn items_should_return_all_fifo_contents_in_order() {
        let mut fifo = Fifo::new(3);
        assert!(fifo.empty());
        fifo.push(10);
        fifo.push(20);
        fifo.push(30);
        fifo.pull();
        assert_eq!(fifo.items(), vec![20, 30]);
    }
}
