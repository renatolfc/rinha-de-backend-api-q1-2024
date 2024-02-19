use std::vec::Vec;

use bytes::{Buf, Bytes};
use chrono::{DateTime, Utc};
use clap::Parser;
use http_body_util::{BodyExt, Full};
use hyper::server::conn::http1;
use hyper::{
    body::Incoming as IncomingBody, service::service_fn, Method, Request, Response, StatusCode,
};
use hyper_util::rt::TokioIo;
use regex::Regex;
use serde::{de, Deserialize, Serialize};
use serde_json;
use sqlx::postgres::PgPool;
use std::net::SocketAddr;
use tokio::net::TcpListener;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

type GenericError = Box<dyn std::error::Error + Send + Sync>;
type BoxBody = http_body_util::combinators::BoxBody<Bytes, hyper::Error>;
type Result<T> = std::result::Result<T, GenericError>;

/// Servidor de API para a Rinha de Backend Q1 2024
#[derive(Parser)]
struct Args {
    /// URI para conectar ao servidor de banco de dados
    #[arg(short, long, default_value_t)]
    dburi: String,
}

#[derive(Debug, Serialize, Deserialize, sqlx::Type, PartialEq)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "tipot")]
enum TipoTransação {
    C,
    D,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
struct Transação {
    valor: i32,
    tipo: TipoTransação,
    descricao: String,
    realizada_em: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Saldo {
    saldo: i32,
    limite: i32,
}

#[derive(Debug, Serialize, Deserialize)]
struct SaldoExtrato {
    total: i32,
    limite: i32,
    data_extrato: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Extrato {
    saldo: SaldoExtrato,
    ultimas_transacoes: Vec<Transação>,
}

#[derive(Serialize, Deserialize)]
struct Usuário {
    total: i32,
    data_extrato: DateTime<Utc>,
    limite: i32,
    ultimas_transacoes: Vec<Transação>,
}

#[inline]
async fn deserialize<T>(req: Request<IncomingBody>) -> Result<T>
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
        Err(_) => {
            return Ok(Response::builder()
                .status(StatusCode::UNPROCESSABLE_ENTITY)
                .body(full("Rota não encontrada"))
                .unwrap());
        }
    };
    if transação.descricao.len() < 1 {
        return Ok(Response::builder()
            .status(StatusCode::UNPROCESSABLE_ENTITY)
            .body(full("Descrição muito curta"))
            .unwrap());
    }

    let mut transaction = match pool.begin().await {
        Ok(transaction) => transaction,
        Err(_) => {
            return Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(full("Deu ruim na hora de iniciar a transação"))
                .unwrap());
        }
    };

    let saldo = match sqlx::query_as!(
        Saldo,
        r#"UPDATE users SET saldo = saldo + $1, updated_at = $2
        WHERE id = $3
        RETURNING saldo, limite
        "#,
        if transação.tipo == TipoTransação::C {
            transação.valor
        } else {
            -transação.valor
        },
        Utc::now(),
        id
    )
    .fetch_one(transaction.as_mut())
    .await
    {
        Ok(saldo) => saldo,
        Err(_) => {
            return Ok(Response::builder()
                .status(StatusCode::UNPROCESSABLE_ENTITY)
                .body(full("SaldoExtrato negativo excede limite"))
                .unwrap());
        }
    };

    match sqlx::query!(
        r#"INSERT INTO ledger (id_cliente, valor, tipo, descricao)
        VALUES ($1, $2, $3, $4)"#,
        id,
        transação.valor,
        transação.tipo as _,
        transação.descricao
    )
    .execute(&mut *transaction)
    .await
    {
        Ok(ledger_insertion) => ledger_insertion,
        Err(_) => {
            return Ok(Response::builder()
                .status(StatusCode::UNPROCESSABLE_ENTITY)
                .body(full("Deu ruim na hora de inserir a transação"))
                .unwrap());
        }
    };
    match transaction.commit().await {
        Ok(_) => (),
        Err(_) => {
            return Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(full("Deu ruim na hora de commitar a transação"))
                .unwrap());
        }
    }

    return Ok(Response::builder()
        .status(StatusCode::OK)
        .body(full(serde_json::to_vec(&saldo).unwrap()))
        .unwrap());
}

async fn dispatcher(pool: PgPool, req: Request<IncomingBody>) -> Result<Response<BoxBody>> {
    let extrato_re = Regex::new(r"/clientes/([0-9]+)/extrato/?")?;
    let transacao_re = Regex::new(r"/clientes/([0-9]+)/transacoes/?")?;
    if req.method() == &Method::GET && extrato_re.is_match(req.uri().path()) {
        let id = extrato_re
            .captures(req.uri().path())
            .unwrap()
            .get(1)
            .unwrap()
            .as_str()
            .parse::<i32>()
            .unwrap();
        return pega_extrato(pool, id).await;
    }
    if req.method() == &Method::POST && transacao_re.is_match(req.uri().path()) {
        let id = transacao_re
            .captures(req.uri().path())
            .unwrap()
            .get(1)
            .unwrap()
            .as_str()
            .parse::<i32>()
            .unwrap();
        return põe_transação(pool, id, req).await;
    }
    let response = Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(full("Rota não encontrada"))
        .unwrap();
    return Ok(response);
}

async fn pega_extrato(pool: PgPool, id: i32) -> Result<Response<BoxBody>> {
    let futuro_saldo = sqlx::query_as!(SaldoExtrato, "SELECT saldo as total, limite, now() at time zone 'utc' as \"data_extrato: DateTime<Utc>\" FROM users WHERE id = $1", id)
        .fetch_one(&pool);

    let transações: Vec<Transação> = match sqlx::query_as!(
        Transação,
        r#"SELECT valor, tipo as "tipo: TipoTransação", descricao, realizada_em from ledger
	    WHERE id_cliente = $1 ORDER BY realizada_em DESC LIMIT 10"#,
        id
    )
    .fetch_all(&pool)
    .await
    {
        Ok(transações) => transações,
        Err(_) => {
            return Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(full("Deu ruim na hora de listar as transações"))
                .unwrap())
        }
    };

    let saldo = match futuro_saldo.await {
        Ok(saldo) => saldo,
        Err(_) => {
            return Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(full(Bytes::from("Deu ruim na hora de pegar o saldo")))
                .unwrap())
        }
    };

    let ret = Extrato {
        saldo: saldo,
        ultimas_transacoes: transações,
    };
    let json = serde_json::to_string(&ret).unwrap();

    return Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .header("Content-Length", json.len() as u64)
        .body(full(json))
        .unwrap());
}

fn full<T: Into<Bytes>>(chunk: T) -> BoxBody {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let addr: SocketAddr = ([127, 0, 0, 1], 9999).into();

    let args = Args::parse();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(16)
        .min_connections(4)
        .connect(args.dburi.as_str())
        .await?;

    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("Não rolou de popular o banco");

    let listener = TcpListener::bind(addr).await?;
    println!("Listening on: {}", addr);

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let pool = pool.clone();

        tokio::task::spawn(async move {
            let service = service_fn(move |req| {
                dispatcher(pool.clone(), req)
            });
            if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
                println!("Failed to serve connection: {:?}", err);
            }
        });
    }
}
