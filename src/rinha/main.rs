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

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[inline]
pub async fn deserialize<T>(req: Request<IncomingBody>) -> Result<T>
where
    for<'de> T: de::Deserialize<'de>,
{
    let whole_body = req.collect().await?.aggregate();
    let body = serde_json::from_reader(whole_body.reader())?;
    Ok(body)
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
    if transação.descricao.len() < 1 {
        return respond!("Descrição muito curta", StatusCode::UNPROCESSABLE_ENTITY);
    }

    let mut saldo_atual: i32 = 0;
    let mut limite_atual: i32 = 0;
    let transação_valor = {
        if transação.tipo == TipoTransação::C {
            transação.valor
        } else {
            -transação.valor
        }
    };
    let saldo = match sqlx::query!(
        "CALL atualiza_livro_caixa($1, $2, $3, $4, $5, $6, $7)",
        id,
        transação.valor,
        transação_valor,
        transação.tipo as _,
        transação.descricao,
        *&mut saldo_atual,
        *&mut limite_atual
    )
    .fetch_one(&pool)
    .await
    {
        Ok(row) => Saldo {
            saldo: row.saldo_atual,
            limite: row.limite_atual,
        },
        Err(_) => {
            return respond!(
                "Saldo negativo excede limite",
                StatusCode::UNPROCESSABLE_ENTITY
            )
        }
    };

    if saldo.saldo.is_none() {
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
    let addr: SocketAddr = ([0, 0, 0, 0], 9999).into();

    let args = Args::parse();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(args.max_pool)
        .min_connections(args.min_pool)
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
