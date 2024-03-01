use bytes::{Buf, Bytes};
use clap::Parser;
use http_body_util::BodyExt;
use hyper::server::conn::http1;
use hyper::{body::Incoming as IncomingBody, service::service_fn, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde::de;
use serde_json;
use sqlx::postgres::PgPool;
use std::net::SocketAddr;
use tokio::net::TcpListener;

use rinha::*;

#[inline]
pub async fn deserialize<T>(req: Request<IncomingBody>) -> Result<T>
where
    for<'de> T: de::Deserialize<'de>,
{
    let whole_body = req.collect().await?.aggregate();
    let body = serde_json::from_reader(whole_body.reader())?;
    Ok(body)
}

#[inline]
async fn credita(id: i32, valor: i32, descricao: &String, pool: &PgPool) -> Saldo {
    match sqlx::query!("CALL poe($1, $2, $3, null, null)", id, valor, descricao)
        .fetch_one(pool)
        .await
        {
            Ok(row) => {
                return Saldo {
                    saldo: row.saldo_atual,
                    limite: row.limite_atual
                }
            }
            Err(e) => {
                println!("Erro ao creditar: {}", e);
                return Saldo {
                    saldo: Some(-1),
                    limite: Some(-1),
                };
            }
        }
}

#[inline]
async fn debita(id: i32, valor: i32, descricao: &String, pool: &PgPool) -> Saldo {
    match sqlx::query!(
        "CALL tira($1, $2, $3, null, null)",
        id,
        valor,
        descricao,
    )
    .fetch_one(pool)
    .await
    {
        Ok(row) => {
            return Saldo {
                saldo: row.saldo_atual,
                limite: row.limite_atual
            }
        }
        Err(e) => {
            println!("Erro ao creditar: {}", e);
            return Saldo {
                saldo: Some(-1),
                limite: Some(-1),
            };
        }
    }
}

async fn põe_transação(
    pool: PgPool,
    id: i32,
    body: Request<IncomingBody>,
) -> Result<Response<BoxBody>> {
    let transação: Transação = match deserialize(body).await {
        Ok(transação) => transação,
        Err(_) => return respond!("Erro ao deserializar", StatusCode::UNPROCESSABLE_ENTITY),
    };
    if transação.descricao.len() < 1 || transação.descricao.len() > 10 {
        return respond!("Descrição muito curta ou muito longa.", StatusCode::UNPROCESSABLE_ENTITY);
    }

    let saldo = {
        if transação.tipo == TipoTransação::C {
            credita(id, transação.valor, &transação.descricao, &pool).await
        } else {
            debita(id, transação.valor, &transação.descricao, &pool).await
        }
    };

    if saldo.saldo == Some(-1) && saldo.limite == Some(-1) {
        return respond!(
            "Saldo negativo excede limite",
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    return respond!(serde_json::to_vec(&saldo).unwrap());
}

async fn dispatcher(pool: PgPool, req: Request<IncomingBody>) -> Result<Response<BoxBody>> {
    let rota = extrai_rota(&req);
    if let (Rota::PEGA, id) = rota {
        return pega_extrato(pool, id).await;
    }
    if let (Rota::PÕE, id) = rota {
        return põe_transação(pool, id, req).await;
    }
    respond!("Rota não encontrada", StatusCode::NOT_FOUND)
}

async fn pega_extrato(pool: PgPool, id: i32) -> Result<Response<BoxBody>> {
    match lê_extrato(pool, id).await {
        Some(extrato) => respond!(serde_json::to_string(&extrato).unwrap()),
        None => respond!("Erro ao ler extrato", StatusCode::INTERNAL_SERVER_ERROR),
    }
}

#[tokio::main(worker_threads = 8)]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let port = std::env::var("PORT").unwrap_or_else(|_| "9999".into());
    let port = port.parse::<u16>().unwrap();
    let addr: SocketAddr = ([0, 0, 0, 0], port).into();

    let args = Args::parse();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(args.max_pool)
        .min_connections(args.min_pool)
        .test_before_acquire(false)
        .max_lifetime(None)
        .idle_timeout(None)
        .connect(args.dburi.as_str())
        .await?;

    let listener = TcpListener::bind(addr).await?;
    println!("Listening on: {}", addr);

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let pool = pool.clone();

        tokio::task::spawn(async move {
            let service = service_fn(move |req| dispatcher(pool.clone(), req));
            if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
                println!("Failed to serve connection: {:?}", err);
            }
        });
    }
}
