use std::net::SocketAddr;

use bytes::Bytes;
use fuso_api::{async_trait, Error, FusoListener, FusoPacket, Packet, Result};

use futures::AsyncWriteExt;
use smol::{
    channel::{unbounded, Receiver},
    net::TcpStream,
};

use crate::cmd::{CMD_BIND, CMD_CREATE, CMD_PING};
use crate::retain::Heartbeat;

#[allow(unused)]
pub struct Reactor {
    addr: SocketAddr,
    packet: Packet,
}

pub struct Fuso {
    accept_ax: Receiver<Reactor>,
}

impl Fuso {
    pub async fn bind(addr: SocketAddr) -> Result<Self> {
        let mut stream = TcpStream::connect(addr)
            .await
            .map_err(|e| Error::with_io(e))?
            .guard(5000)
            .await?;

        stream.send(Packet::new(CMD_BIND, Bytes::new())).await?;

        let (accept_tx, accept_ax) = unbounded();

        smol::spawn(async move {
            loop {
                match stream.recv().await {
                    Err(e) => {
                        log::warn!("[fuc] Server disconnect {}", e);
                        break;
                    }
                    Ok(packet) if packet.get_cmd() == CMD_PING => {}
                    Ok(packet) => {
                        if let Err(_) = accept_tx.send(Reactor { addr, packet }).await {
                            let _ = stream.close().await;
                            break;
                        };
                    }
                }
            }
        })
        .detach();

        Ok(Self { accept_ax })
    }
}

impl Reactor {
    #[inline]
    pub async fn join(self) -> Result<TcpStream> {
        let mut stream = TcpStream::connect(self.addr)
            .await
            .map_err(|e| Error::with_io(e))?;

        stream.send(Packet::new(CMD_CREATE, Bytes::new())).await?;

        Ok(stream)
    }
}

#[async_trait]
impl FusoListener<Reactor> for Fuso {
    #[inline]
    async fn accept(&mut self) -> Result<Reactor> {
        Ok(self.accept_ax.recv().await.map_err(|e| {
            Error::with_io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        })?)
    }

    #[inline]
    async fn close(&mut self) -> Result<()> {
        self.accept_ax.close();
        Ok(())
    }
}
