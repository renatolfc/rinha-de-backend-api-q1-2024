use std::env;
use std::net::{SocketAddr, ToSocketAddrs};

use hyper::{server::conn::http1, service::service_fn};
use hyper_util::rt::TokioIo;
use tokio::net::{TcpListener, TcpStream};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[inline]
fn split_str(input: &str) -> Vec<String> {
    input.split(',').map(|s| s.to_string()).collect()
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
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

    let in_addr: SocketAddr = ([0, 0, 0, 0], 1108).into();
    let listener = TcpListener::bind(in_addr).await?;
    println!("Listening on {}", in_addr);
    let mut counter = 0;

    loop {
        counter = (counter + 1) % inet_addrs.len();
        let out_addr = inet_addrs[counter];

        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);

        let service = service_fn(move |mut req| {
            let uri_string = format!(
                "http://{}{}",
                out_addr,
                req.uri()
                    .path_and_query()
                    .map(|x| x.as_str())
                    .unwrap_or("/")
            );
            let uri = uri_string.parse().unwrap();
            *req.uri_mut() = uri;

            let host = req.uri().host().expect("uri has no host");
            let port = req.uri().port_u16().unwrap_or(80);
            let addr = format!("{}:{}", host, port);

            async move {
                let client_stream = TcpStream::connect(addr).await.unwrap();
                let io = TokioIo::new(client_stream);

                let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;
                tokio::task::spawn(async move {
                    if let Err(err) = conn.await {
                        println!("Connection failed: {:?}", err);
                    }
                });

                sender.send_request(req).await
            }
        });

        tokio::task::spawn(async move {
            if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
                println!("Failed to serve the connection: {:?}", err);
            }
        });
    }
}
