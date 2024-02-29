use std::env;

use bytes::Bytes;
use chrono::{DateTime, Utc};
use clap::Parser;
use http_body_util::{BodyExt, Full};
use hyper::{body::Incoming as IncomingBody, Method, Request, Response, StatusCode};
use regex::Regex;
use serde::{Deserialize, Serialize};

pub type GenericError = Box<dyn std::error::Error + Send + Sync>;
pub type BoxBody = http_body_util::combinators::BoxBody<Bytes, hyper::Error>;
pub type Result<T> = std::result::Result<T, GenericError>;

/// Servidor de API para a Rinha de Backend Q1 2024
#[derive(Parser)]
pub struct Args {
    /// URI para conectar ao servidor de banco de dados
    #[arg(short, long, default_value_t)]
    pub dburi: String,

    /// Tamanho minimo do pool de conexões
    #[arg(short, long, default_value_t)]
    pub min_pool: u32,

    /// Tamanho maximo do pool de conexões
    #[arg(short, long, default_value_t)]
    pub max_pool: u32,
}

impl Default for Args {
    fn default() -> Args {
        Args {
            dburi: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://postgres:postgres@postgres/rinha".into()),
            min_pool: env::var("MIN_POOL")
                .unwrap_or_else(|_| "1".into())
                .parse()
                .unwrap(),
            max_pool: env::var("MAX_POOL")
                .unwrap_or_else(|_| "10".into())
                .parse()
                .unwrap(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, sqlx::Type, PartialEq)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "tipot")]
pub enum TipoTransação {
    C,
    D,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Transação {
    pub valor: i32,
    pub tipo: TipoTransação,
    pub descricao: String,
    pub realizada_em: Option<DateTime<Utc>>,
}

impl Clone for Transação {
    fn clone(&self) -> Self {
        let tipo = match self.tipo {
            TipoTransação::C => TipoTransação::C,
            TipoTransação::D => TipoTransação::D,
        };
        Transação {
            valor: self.valor,
            tipo,
            descricao: self.descricao.clone(),
            realizada_em: self.realizada_em,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Saldo {
    pub saldo: Option<i32>,
    pub limite: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SaldoExtrato {
    pub total: i32,
    pub limite: i32,
    pub data_extrato: Option<DateTime<Utc>>,
}

impl Clone for SaldoExtrato {
    fn clone(&self) -> Self {
        SaldoExtrato {
            total: self.total,
            limite: self.limite,
            data_extrato: self.data_extrato,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Extrato {
    pub saldo: SaldoExtrato,
    pub ultimas_transacoes: Vec<Transação>,
}

impl Default for Extrato {
    fn default() -> Extrato {
        Extrato {
            saldo: SaldoExtrato {
                total: 0,
                limite: 0,
                data_extrato: Some(DateTime::from(Utc::now())),
            },
            ultimas_transacoes: Vec::new(),
        }
    }
}

impl Clone for Extrato {
    fn clone(&self) -> Self {
        Extrato {
            saldo: self.saldo.clone(),
            ultimas_transacoes: self.ultimas_transacoes.clone(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Usuário {
    pub total: i32,
    pub data_extrato: DateTime<Utc>,
    pub limite: i32,
    pub ultimas_transacoes: Vec<Transação>,
}

#[derive(PartialEq)]
pub enum Rota {
    NENHUMA,
    PEGA,
    PÕE,
}

pub fn extrai_rota(req: &Request<IncomingBody>) -> (Rota, i32) {
    let rota_re = Regex::new(r"/clientes/([0-9]+)/(extrato|transacoes)/?").unwrap();
    if rota_re.is_match(req.uri().path()) {
        let id = rota_re
            .captures(req.uri().path())
            .unwrap()
            .get(1)
            .unwrap()
            .as_str()
            .parse::<i32>()
            .unwrap();
        let rota = rota_re
            .captures(req.uri().path())
            .unwrap()
            .get(2)
            .unwrap()
            .as_str();
        if req.method() == &Method::GET && rota == "extrato" && id > 0 && id < 6 {
            return (Rota::PEGA, id);
        }
        if req.method() == &Method::POST && rota == "transacoes" && id > 0 && id < 6 {
            return (Rota::PÕE, id);
        }
    }
    (Rota::NENHUMA, 0)
}

#[inline]
pub async fn lê_extrato(pool: sqlx::PgPool, id: i32) -> Option<Extrato> {
    let saldo = match sqlx::query_as!(
        SaldoExtrato,
        r#"SELECT saldo as total, limite, now() at time zone 'utc' as "data_extrato: DateTime<Utc>"
        FROM users
        WHERE id = $1"#,
        id
    )
    .fetch_one(&pool)
    .await
    {
        Ok(saldo) => saldo,
        Err(_) => {
            return None;
        }
    };

    let transações: Vec<Transação> = match sqlx::query_as!(
        Transação,
        r#"SELECT valor, tipo as "tipo: TipoTransação", descricao, realizada_em from ledger
	    WHERE id_cliente = $1 ORDER BY realizada_em DESC LIMIT 10"#,
        id
    )
    .fetch_all(&pool)
    .await
    {
        Ok(transações) => Vec::from(transações),
        Err(_) => {
            return None;
        }
    };

    Some(Extrato {
        saldo,
        ultimas_transacoes: transações,
    })
}

#[inline]
pub fn full<T: Into<Bytes>>(chunk: T) -> BoxBody {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}

#[inline]
pub fn respond(body: Bytes, status: StatusCode) -> Result<Response<BoxBody>> {
    Ok(Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .header("Content-Length", body.len() as u64)
        .body(full(body))
        .unwrap())
}

#[macro_export]
macro_rules! respond {
    ($body:expr) => {
        respond(Bytes::from($body), StatusCode::OK)
    };
    ($body:expr, $status:expr) => {
        respond(Bytes::from($body), $status)
    };
}
