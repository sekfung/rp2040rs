use std::cell::RefCell;
use std::rc::Rc;

use rp2040rs::rp2040::Rp2040;

const UART0: u32 = 0x4003_4000;
const DR: u32 = UART0;
const FR: u32 = UART0 + 0x18;
const LCR_H: u32 = UART0 + 0x2c;
const CR: u32 = UART0 + 0x30;
const IMSC: u32 = UART0 + 0x38;
const RIS: u32 = UART0 + 0x3c;
const MIS: u32 = UART0 + 0x40;
const ICR: u32 = UART0 + 0x44;

const UARTEN: u32 = 1;
const TXE: u32 = 1 << 8;
const RXE: u32 = 1 << 9;
const FEN: u32 = 1 << 4;
const RXINTR: u32 = 1 << 4;
const TXINTR: u32 = 1 << 5;
const RXFE: u32 = 1 << 4;

#[test]
fn uart_receives_data_and_raises_rx_interrupt() {
    let rp = Rp2040::new();
    rp.write_u32(CR, UARTEN | TXE | RXE);
    rp.write_u32(IMSC, RXINTR);
    rp.uart[0].feed_byte(0x41);

    assert_eq!(rp.read_u32(RIS) & RXINTR, RXINTR);
    assert_eq!(rp.read_u32(MIS) & RXINTR, RXINTR);
    assert_ne!(rp.core.pending_interrupts() & (1 << 20), 0);
    assert_eq!(rp.read_u32(DR), 0x41);
    assert_eq!(rp.read_u32(FR) & RXFE, RXFE);
    assert_eq!(rp.read_u32(RIS) & RXINTR, 0);
}

#[test]
fn uart_transmit_callback_and_fifo_enabled_line_callback() {
    let rp = Rp2040::new();
    rp.write_u32(CR, UARTEN | TXE | RXE);
    let bytes = Rc::new(RefCell::new(Vec::new()));
    let seen = bytes.clone();
    rp.uart[0].set_on_byte(Box::new(move |value| seen.borrow_mut().push(value)));
    rp.write_u32(DR, 0x55);
    assert_eq!(*bytes.borrow(), vec![0x55]);

    rp.write_u32(LCR_H, FEN);
    let line = Rc::new(RefCell::new(String::new()));
    let captured = line.clone();
    rp.uart[0].set_on_line(Box::new(move |value| *captured.borrow_mut() = value));
    for byte in b"hello\n" {
        rp.uart[0].feed_byte(*byte);
    }
    assert_eq!(&*line.borrow(), "hello");
}

#[test]
fn uart_tx_interrupt_and_peripheral_ids() {
    let rp = Rp2040::new();
    rp.write_u32(CR, UARTEN | TXE | RXE);
    rp.write_u32(IMSC, TXINTR);
    assert_ne!(rp.read_u32(RIS) & TXINTR, 0);
    assert_ne!(rp.core.pending_interrupts() & (1 << 20), 0);
    rp.write_u32(ICR, TXINTR);
    assert_eq!(rp.read_u32(RIS) & TXINTR, 0);
    assert_eq!(rp.read_u32(UART0 + 0xfe0), 0x11);
    assert_eq!(rp.read_u32(UART0 + 0xff0), 0x0d);
}
