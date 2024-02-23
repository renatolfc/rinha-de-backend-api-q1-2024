use std::env;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::Arc;

use async_std::sync::{Mutex, RwLock};
use bytes::Bytes;
use http_body_util::BodyExt;
use hyper::{
    body::Incoming, server::conn::http1, service::service_fn, Request, Response, StatusCode,
};
use hyper_util::rt::TokioIo;
use tokio::net::{TcpListener, TcpStream};

use rinha::*;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

type DentroDoDb = Arc<RwLock<Vec<u8>>>;
type Db = Vec<DentroDoDb>;
type Foguinho = Arc<Mutex<bool>>;
type Labareda = Vec<Foguinho>;

macro_rules! vec_no_clone {
    ( $val:expr; $n:expr ) => {{
        let result: Vec<_> = std::iter::repeat_with(|| $val).take($n).collect();
        result
    }};
}

#[inline]
fn split_str(input: &str) -> Vec<String> {
    input.split(',').map(|s| s.to_string()).collect()
}

#[inline]
async fn encaminha(
    req: Request<Incoming>,
    out_addr: SocketAddr,
) -> std::result::Result<Response<hyper::body::Incoming>, hyper::Error> {
    let uri_string = format!(
        "http://{}{}",
        out_addr.clone(),
        req.uri()
            .path_and_query()
            .map(|x| x.as_str())
            .unwrap_or("/")
    );
    let (parts, body) = req.into_parts();
    let mut req = Request::from_parts(parts, body);
    let uri = uri_string.parse().unwrap();
    *req.uri_mut() = uri;

    let host = req.uri().host().expect("uri has no host");
    let port = req.uri().port_u16().unwrap_or(80);
    let addr = format!("{}:{}", host, port);
    let client_stream = TcpStream::connect(addr).await.unwrap();
    let io = TokioIo::new(client_stream);

    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;
    tokio::task::spawn(async move {
        if let Err(err) = conn.await {
            println!("Failed to send request: {:?}", err);
        }
    });

    sender.send_request(req).await
}

#[inline]
async fn dispatcher(
    req: Request<Incoming>,
    cache: DentroDoDb,
    tá_quente: Foguinho,
    rota: Rota,
    out_addr: SocketAddr,
) -> Result<Response<BoxBody>> {
    let quente = tá_quente.lock().await;
    if rota == Rota::PEGA {
        if *quente {
            let e = cache.read().await;
            return rinha::respond(e.clone().into(), StatusCode::OK);
        }
        drop(quente);
        let res = encaminha(req, out_addr).await?;
        if res.status() != StatusCode::OK {
            return rinha::respond!("Erro ao ler extrato", res.status());
        }
        let (parts, body) = res.into_parts();
        let body: Vec<u8> = body.collect().await?.to_bytes().into();
        let mut e = cache.write().await;
        *e = body.clone();
        let mut quente = tá_quente.lock().await;
        *quente = true;
        drop(quente);
        return rinha::respond(body.into(), parts.status);
    }
    if rota == Rota::PÕE {
        drop(quente);
        let res = encaminha(req, out_addr).await?;
        if res.status() != StatusCode::OK {
            return rinha::respond!("Erro ao gravar transação", res.status());
        }
        let mut quente = tá_quente.lock().await;
        *quente = false;
        drop(quente);
        let (parts, body) = res.into_parts();
        let body: Vec<u8> = body.collect().await?.to_bytes().into();
        return rinha::respond(body.into(), parts.status);
    }
    return rinha::respond!("Rota não encontrada", StatusCode::NOT_FOUND);
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let servers = split_str(
        &env::var("SERVERS").unwrap_or_else(|_| "api01:9999,api02:9999".into()),
    );
    println!("Servers: {:?}", servers);

    let inet_addrs: Vec<SocketAddr> = servers
        .iter()
        .map(|s| s.to_socket_addrs().expect("Unable to resolve socket address for server"))
        .flatten()
        .collect();

    let cache: Db = vec_no_clone!(Arc::new(RwLock::new(Vec::new())); 5);
    let tá_quente: Labareda = vec_no_clone!(Arc::new(Mutex::new(false)); 5);

    let in_addr: SocketAddr = ([0, 0, 0, 0], 1108).into();
    let listener = TcpListener::bind(in_addr).await?;
    println!("Listening on {}", in_addr);
    let mut counter = 0;

    loop {
        let (stream, _) = listener.accept().await?;
		let io = TokioIo::new(stream);
        // randomly select target inet address
        counter = (counter + 1) % inet_addrs.len();
        let out_addr = inet_addrs[counter].clone();
        let cache = cache.clone();
        let tá_quente = tá_quente.clone();

        let service = service_fn(move |req| {
            let (rota, mut id) = rinha::extrai_rota(&req);
            if rota != Rota::NENHUMA {
                id -= 1;
            }
            let cache = cache[id as usize].clone();
            let tá_quente = tá_quente[id as usize].clone();
            dispatcher(
                req,
                cache,
                tá_quente,
                rota,
                out_addr,
            )
        });

        tokio::task::spawn(async move {
            if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
                println!("Failed to serve the connection: {:?}", err);
            }
        });
    }
}
