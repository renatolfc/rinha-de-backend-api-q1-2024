#[macro_use] extern crate rocket;

use std::vec::Vec;

use clap::Parser;
use chrono::{DateTime,Utc};
use chrono::serde::ts_seconds_option;
use rocket::serde::{Deserialize,json::Json};
use sqlx::postgres::PgPool;

static DEFAULT_DB_URI: &'static str = "postgres://postgres:postgres@localhost/rinha";

/// Servidor de API para a Rinha de Backend Q1 2024
#[derive(Parser)]
struct Args {
    /// URI to use to connect to database server
    #[arg(short, long, default_value_t)]
    dburi: String,
}

#[derive(Deserialize)]
#[serde(crate = "rocket::serde")]
#[serde(rename_all = "snake_case")]
enum TipoTransação {
    C,
    D
}

#[derive(Deserialize)]
#[serde(crate = "rocket::serde")]
struct Transação {
    valor: String,
    tipo: TipoTransação,
    descricao: String,
    #[serde(with = "ts_seconds_option")]
    realizado_em: Option<DateTime<Utc>>,
}

struct Extrato {
    total: isize,
    data_extrato: DateTime<Utc>,
    limite: isize,
    ultimas_transacoes: Vec<Transação>,
}


#[get("/")]
fn index() -> &'static str {
    "Hello, world!"
}

#[post("/clientes/<id>/transacoes", data="<trans>")]
fn põe_transação(id: isize, trans: Json<Transação>) -> () {
}

#[get("/clientes/<id>/extrato")]
fn pega_extrato(id: isize) -> () {
}

#[launch]
async fn rocket() -> _ {
    let args = Args::parse();
    let pool = sqlx::PgPool::connect(args.dburi.as_str())
        .await
        .expect("Não rolou de conectar ao banco");

    sqlx::migrate!().run(&pool).await?;

    rocket::build()
        .manage::<PgPool>(pool)
        .mount("/", routes![index])

}
