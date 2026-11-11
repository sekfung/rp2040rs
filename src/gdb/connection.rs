use crate::gdb::server::{GdbServer, STOP_REPLY_SIGINT};
use crate::gdb::utils::{gdb_checksum, gdb_message};

#[derive(Default)]
pub struct GdbConnection {
    buffer: String,
}

impl GdbConnection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed_data(&mut self, data: &str, server: &mut GdbServer) -> Vec<String> {
        let mut responses = Vec::new();
        let mut data = data;
        if data.starts_with('\u{3}') {
            server.target.stop();
            responses.push(gdb_message(STOP_REPLY_SIGINT));
            data = &data[1..];
        }
        self.buffer.push_str(data);
        while let Some(dollar) = self.buffer.find('$') {
            let Some(relative_hash) = self.buffer[dollar + 1..].find('#') else {
                break;
            };
            let hash = dollar + 1 + relative_hash;
            if hash + 3 > self.buffer.len() {
                break;
            }
            let command = self.buffer[dollar + 1..hash].to_string();
            let checksum = self.buffer[hash + 1..hash + 3].to_string();
            self.buffer.drain(..hash + 3);
            if gdb_checksum(&command) != checksum {
                responses.push("-".into());
                continue;
            }
            responses.push("+".into());
            if let Some(response) = server.process_message(&command) {
                responses.push(gdb_message(&response));
            }
        }
        responses
    }
}
