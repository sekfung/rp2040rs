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
            self.serve_one(server)?;
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
            let count = stream.read(&mut buffer)?;
            if count == 0 {
                return Ok(());
            }
            let data = String::from_utf8_lossy(&buffer[..count]);
            for response in connection.feed_data(&data, server) {
                stream.write_all(response.as_bytes())?;
            }
            stream.flush()?;
        }
    }
}
