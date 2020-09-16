use std::{error::Error, net::SocketAddr};
use tokio::net::TcpStream;

pub enum ProxyChainsMode {
    Dynamic,
    Strict,
    Random,
}

pub struct ProxyChainsConf {
    pub _mode: ProxyChainsMode,
}

pub struct ProxyChains {}

impl ProxyChains {
    pub async fn connect(
        target_addr: SocketAddr,
        _conf: ProxyChainsConf,
    ) -> Result<TcpStream, Box<dyn Error + Send>> {
        // TODO: implement different modes
        // TODO: remove .unwrap()
        let stream = TcpStream::connect(target_addr).await.unwrap();
        Ok(stream)
    }
}
