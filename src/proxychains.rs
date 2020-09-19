use rand::seq::SliceRandom;
use serde_derive::Deserialize;
use socks5_async::connect_with_stream;
use std::{error::Error, fs::File, io::Read, net::SocketAddr};
use tokio::net::TcpStream;

#[derive(Debug, Deserialize, Clone)]
pub struct Proxy {
    pub socket_addr: SocketAddr,
    pub auth: Option<(String, String)>,
}

impl PartialEq for Proxy {
    fn eq(&self, other: &Self) -> bool {
        self.socket_addr == other.socket_addr
    }
}

#[derive(Debug, Deserialize)]
pub enum ProxyChainsMode {
    Dynamic,
    Strict,
    Random,
}

#[derive(Debug, Deserialize)]
pub struct ProxyChainsConf {
    pub mode: ProxyChainsMode,
    pub proxies: Vec<Proxy>,
    pub chain_len: usize,
}
impl ProxyChainsConf {
    pub fn from_file(path: &str) -> Result<Self, Box<dyn Error>> {
        let mut file = File::open(path)?;
        let mut content = String::from("");
        file.read_to_string(&mut content)?;
        let conf: ProxyChainsConf = toml::from_str(&content).expect("Failed to parse");
        Ok(conf)
    }
}

pub struct ProxyChains {}
impl ProxyChains {
    pub async fn connect(
        target_addr: SocketAddr,
        conf: &ProxyChainsConf,
    ) -> Result<TcpStream, Box<dyn Error + Send + Sync>> {
        // validate the number of proxies
        if conf.proxies.len() < 1 {
            Err("No proxies provided.")?;
        }

        let stream = match conf.mode {
            ProxyChainsMode::Strict => strict(target_addr, &conf).await.fix_box()?,
            ProxyChainsMode::Random => random(target_addr, &conf).await.fix_box()?,

            // TODO: implement this method
            ProxyChainsMode::Dynamic => TcpStream::connect(target_addr).await?,
        };

        Ok(stream)
    }
}

// Strict proxychains stream generator
async fn strict(
    target_addr: SocketAddr,
    conf: &ProxyChainsConf,
) -> Result<TcpStream, Box<dyn Error>> {
    let first = conf.proxies.get(0).unwrap();
    let mut stream = TcpStream::connect(first.socket_addr).await?;
    connect_with_stream(
        &mut stream,
        *strict_next_addr(&target_addr, &conf.proxies, 0),
        first.auth.clone(),
    )
    .await?;

    let mut i = 1;
    for proxy in &conf.proxies[1..] {
        connect_with_stream(
            &mut stream,
            *strict_next_addr(&target_addr, &conf.proxies, i),
            proxy.auth.clone(),
        )
        .await?;
        i += 1;
    }

    Ok(stream)
}
// Get the next target address
// If there's a proxy left in the chain, the next proxy address will be returned
// If there's no proxy left, target address will be returned
fn strict_next_addr<'a>(
    target_addr: &'a SocketAddr,
    proxies: &'a Vec<Proxy>,
    current: usize,
) -> &'a SocketAddr {
    if proxies.len() - 1 - current > 0 {
        &proxies.get(current + 1).unwrap().socket_addr
    } else {
        target_addr
    }
}

// Random proxychains stream generator
async fn random(
    target_addr: SocketAddr,
    conf: &ProxyChainsConf,
) -> Result<TcpStream, Box<dyn Error>> {
    if conf.chain_len > conf.proxies.len() {
        Err("chain_len is greater than the number of proxies.")?;
    }
    if conf.chain_len < 1 {
        Err("chain_len is 0 !")?;
    }

    let selection: Vec<_> = conf
        .proxies
        .choose_multiple(&mut rand::thread_rng(), conf.chain_len)
        .map(|x| x.clone())
        .collect();

    let new_config = ProxyChainsConf {
        chain_len: conf.chain_len,
        proxies: selection,
        mode: ProxyChainsMode::Strict,
    };

    strict(target_addr, &new_config).await
}

// We are currently unable to convert a Box<dyn Error>
// to Box<dyn Error + Send + Sync>> using ?
// This helps to do this conversion
// TODO: find a better way
trait FixBoxError<T> {
    fn fix_box(self) -> Result<T, Box<dyn Error + Send + Sync>>;
}
impl<T> FixBoxError<T> for Result<T, Box<dyn Error>> {
    fn fix_box(self) -> Result<T, Box<dyn Error + Send + Sync>> {
        match self {
            Err(err) => Err(err.to_string().into()),
            Ok(t) => Ok(t),
        }
    }
}
