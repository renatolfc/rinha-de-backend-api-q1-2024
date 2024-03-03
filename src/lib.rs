use std::env;

use bytes::Bytes;
use chrono::{DateTime, Utc};
use clap::Parser;
use http_body_util::{BodyExt, Full};
use hyper::{body::Incoming as IncomingBody, Method, Request, Response, StatusCode};
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

#[derive(Debug, Serialize, Deserialize)]
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

impl From<&TransaçãoBanco> for Transação {
    fn from(transação: &TransaçãoBanco) -> Self {
        let tipo = match transação.tipo {
            TipoTransação::C => TipoTransação::C,
            TipoTransação::D => TipoTransação::D,
        };
        Transação {
            valor: transação.valor,
            tipo: tipo,
            descricao: transação.descricao.clone(),
            realizada_em: transação.realizada_em,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct TransaçãoBanco {
    pub saldo: i32,
    pub limite: i32,
    pub data_extrato: Option<DateTime<Utc>>,
    pub valor: i32,
    pub tipo: TipoTransação,
    pub descricao: String,
    pub realizada_em: Option<DateTime<Utc>>,
}

impl Clone for TransaçãoBanco {
    fn clone(&self) -> Self {
        let tipo = match self.tipo {
            TipoTransação::C => TipoTransação::C,
            TipoTransação::D => TipoTransação::D,
        };
        TransaçãoBanco {
            saldo: self.saldo,
            limite: self.limite,
            data_extrato: self.data_extrato,
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
    let path = req.uri().path();
    let id_potencial: Option<&str> = path.get(10 as usize..11 as usize);
    if id_potencial.is_none() {
        return (Rota::NENHUMA, 0);
    }
    let id = id_potencial.unwrap().parse::<i32>();
    if id.is_err() {
        return (Rota::NENHUMA, 0);
    }
    let id = id.unwrap();
    if !path.starts_with("/clientes/") {
        return (Rota::NENHUMA, 0);
    }
    if req.method() == &Method::GET && path.ends_with("/extrato") && id > 0 && id < 6 {
        return (Rota::PEGA, id);
    }
    if req.method() == &Method::POST && path.ends_with("/transacoes") && id > 0 && id < 6 {
        return (Rota::PÕE, id);
    }
    (Rota::NENHUMA, 0)
}

#[inline]
pub async fn lê_extrato(pool: sqlx::PgPool, id: i32) -> Option<Extrato> {
    let transações: Vec<TransaçãoBanco> = match sqlx::query_as!(
        TransaçãoBanco,
        r#"
        SELECT
            users.saldo as saldo,
            users.limite as limite,
            now() at time zone 'utc' as "data_extrato: DateTime<Utc>",
            ledger.valor as valor,
            ledger.tipo as "tipo: TipoTransação",
            ledger.descricao as descricao,
            ledger.realizada_em as "realizada_em: Option<DateTime<Utc>>"
        FROM users
        LEFT JOIN 
            ledger ON ledger.id_cliente = users.id
        WHERE
            users.id = $1 AND
            ledger.realizada_em <= users.atualizado_em
        ORDER BY
            ledger.realizada_em DESC
        LIMIT 10
        "#,
        id
    )
    .fetch_all(&pool)
    .await
    {
        Ok(transações) => Vec::from(transações),
        Err(_) => {
            [].to_vec()
        }
    };

    if transações.is_empty() {
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
        return Some(Extrato {
            saldo,
            ultimas_transacoes: [].to_vec(),
        });
    }

    let saldo = SaldoExtrato {
        total: transações[0].saldo,
        limite: transações[0].limite,
        data_extrato: transações[0].data_extrato,
    };

    Some(Extrato {
        saldo,
        ultimas_transacoes: transações.iter().map(|t| Transação::from(t)).collect(),
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
