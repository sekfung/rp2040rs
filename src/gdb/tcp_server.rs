use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};

use crate::gdb::connection::GdbConnection;
use crate::gdb::server::GdbServer;

pub struct GdbTcpServer {
    listener: TcpListener,
}

impl GdbTcpServer {
    pub fn bind(port: u16) -> std::io::Result<Self> {
        Ok(Self {
            listener: TcpListener::bind(("127.0.0.1", port))?,
        })
    }

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    pub fn serve_one(&self, server: &mut GdbServer) -> std::io::Result<()> {
        let (mut stream, _) = self.listener.accept()?;
        stream.set_nodelay(true)?;
        self.handle_connection(&mut stream, server)
    }

    pub fn serve(&self, server: &mut GdbServer) -> std::io::Result<()> {
        loop {
            match self.serve_one(server) {
                Ok(()) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::Interrupted
                            | std::io::ErrorKind::ConnectionAborted
                            | std::io::ErrorKind::ConnectionReset
                    ) =>
                {
                    continue;
                }
                Err(error) => return Err(error),
            }
        }
    }

    fn handle_connection(
        &self,
        stream: &mut TcpStream,
        server: &mut GdbServer,
    ) -> std::io::Result<()> {
        let mut connection = GdbConnection::new();
        let mut buffer = [0u8; 1024];
        loop {
            let count = match stream.read(&mut buffer) {
                Ok(count) => count,
                Err(error) if is_disconnect(&error) => return Ok(()),
                Err(error) => return Err(error),
            };
            if count == 0 {
                return Ok(());
            }
            let data = String::from_utf8_lossy(&buffer[..count]);
            for response in connection.feed_data(&data, server) {
                if let Err(error) = stream.write_all(response.as_bytes()) {
                    if is_disconnect(&error) {
                        return Ok(());
                    }
                    return Err(error);
                }
            }
            if let Err(error) = stream.flush() {
                if is_disconnect(&error) {
                    return Ok(());
                }
                return Err(error);
            }
        }
    }
}

fn is_disconnect(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::BrokenPipe
            | std::io::ErrorKind::UnexpectedEof
            | std::io::ErrorKind::NotConnected
    )
}
