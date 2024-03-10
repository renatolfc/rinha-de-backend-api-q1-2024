use std::env;
use std::net::{SocketAddr, ToSocketAddrs};

use async_trait::async_trait;
use deadpool::managed;
use tokio::io::AsyncWriteExt;
use tokio::{
    io,
    net::{TcpListener, TcpStream},
};

static mut COUNTER: usize = 0;

struct Manager {
    servers: Vec<SocketAddr>,
}

#[async_trait]
impl managed::Manager for Manager {
    type Type = TcpStream;
    type Error = io::Error;

    async fn create(&self) -> Result<Self::Type, Self::Error> {
        unsafe {
            COUNTER = (COUNTER + 1) % self.servers.len();
            let server = self.servers[COUNTER];
            let stream = TcpStream::connect(server).await.unwrap();
            stream.set_nodelay(true).expect("set_nodelay call failed");

            Ok(stream)
        }
    }

    async fn recycle(
        &self,
        _conn: &mut Self::Type,
        _: &managed::Metrics,
    ) -> managed::RecycleResult<Self::Error> {
        let (_, mut write) = _conn.split();
        write.shutdown().await?;
        Ok(())
    }
}

type Pool = managed::Pool<Manager>;

#[inline]
fn split_str(input: &str) -> Vec<String> {
    input.split(',').map(|s| s.to_string()).collect()
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<()> {
    let servers =
        split_str(&env::var("SERVERS").unwrap_or_else(|_| "api01:9999,api02:9999".into()));
    println!("Servers: {:?}", servers);

    let inet_addrs: Vec<SocketAddr> = servers
        .iter()
        .map(|s| {
            s.to_socket_addrs()
                .expect("Unable to resolve socket address for server")
        })
        .flatten()
        .collect();

    let port = std::env::var("PORT").unwrap_or_else(|_| "1108".into());
    let port = port.parse::<u16>().unwrap();
    let in_addr: SocketAddr = ([0, 0, 0, 0], port).into();
    let listener = TcpListener::bind(in_addr).await?;
    let pool = Pool::builder(Manager {
        servers: inet_addrs.clone(),
    })
    .max_size(32)
    .build()
    .unwrap();
    println!("Listening on {}", in_addr);

    let mut counter = 0;
    while let Ok((mut downstream, _)) = listener.accept().await {
        counter = (counter + 1) % inet_addrs.len();
        let pool = pool.clone();

        tokio::spawn(async move {
            let mut upstream = pool.get().await.unwrap();
            let mut upstream = upstream.as_mut();
            io::copy_bidirectional(&mut downstream, &mut upstream)
                .await
                .unwrap();
        });
    }

    Ok(())
}
