#[macro_use]
extern crate rocket;

use std::vec::Vec;

use chrono::{DateTime, Utc};
use clap::Parser;
use rocket::http::Status;
use rocket::response::status;
use rocket::serde::{json::Json, Deserialize, Serialize};
use rocket::Request;
use sqlx::postgres::PgPool;

static DEFAULT_DB_URI: &'static str = "postgres://postgres:postgres@localhost/rinha";

/// Servidor de API para a Rinha de Backend Q1 2024
#[derive(Parser)]
struct Args {
    /// URI para conectar ao servidor de banco de dados
    #[arg(short, long, default_value_t)]
    dburi: String,
}

#[derive(Debug, Serialize, Deserialize, sqlx::Type, PartialEq)]
#[serde(crate = "rocket::serde")]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "tipot")]
enum TipoTransação {
    C,
    D,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
#[serde(crate = "rocket::serde")]
struct Transação {
    valor: i32,
    tipo: TipoTransação,
    descricao: String,
    realizada_em: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
struct Saldo {
    saldo: i32,
    limite: i32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
struct Extrato {
    total: i32,
    data_extrato: DateTime<Utc>,
    limite: i32,
    ultimas_transacoes: Vec<Transação>,
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
struct Usuário {
    total: i32,
    data_extrato: DateTime<Utc>,
    limite: i32,
    ultimas_transacoes: Vec<Transação>,
}

#[catch(404)]
fn not_found() -> status::Custom<String> {
    status::Custom(Status::NotFound, "resource not found".to_string())
}

#[catch(422)]
fn unprocessable() -> status::Custom<String> {
    status::Custom(
        Status::UnprocessableEntity,
        "resource not found".to_string(),
    )
}

#[catch(default)]
fn default_catcher(status: Status, req: &Request<'_>) -> status::Custom<String> {
    let msg = format!("{} ({})", status, req.uri());
    status::Custom(Status::UnprocessableEntity, msg)
}

#[post("/clientes/<id>/transacoes", data = "<trans>")]
async fn põe_transação(
    pool: &rocket::State<PgPool>,
    id: i32,
    trans: Json<Transação>,
) -> Result<Json<Saldo>, Status> {
    if id < 1 || id > 5 {
        return Err(Status::NotFound);
    }
    if trans.descricao.len() < 1 {
	return Err(Status::UnprocessableEntity);
    }

    let mut transaction = pool
        .begin()
        .await
        .expect("Não rolou de iniciar transação 🫠");

    let ledger_insertion = match sqlx::query!(
        r#"INSERT INTO ledger (id_cliente, valor, tipo, descricao)
	VALUES ($1, $2, $3, $4)
	"#,
        id,
        trans.valor,
        trans.tipo as _,
        trans.descricao
    )
    .execute(&mut *transaction)
    .await
    {
        Ok(_) => 1,
        Err(_) => 0,
    };

    if ledger_insertion < 1 {
        return Err(Status::NotFound);
    }

    let update = match sqlx::query!(
        r#"UPDATE users SET saldo = saldo + $1, updated_at = $2
	WHERE id = $3
	"#,
        if trans.tipo == TipoTransação::C {
            trans.valor
        } else {
            -trans.valor
        },
        Utc::now(),
        id
    )
    .execute(&mut *transaction)
    .await
    {
	Ok(_) => 1,
	Err(_) => 0,
    };

    if update < 1 {
        return Err(Status::UnprocessableEntity);
    }

    transaction
        .commit()
        .await
        .expect("Não commitou a transação");

    let ret = sqlx::query_as!(Saldo, "SELECT saldo, limite FROM users WHERE id = $1", id)
        .fetch_one(&**pool)
        .await
        .expect("WAT?!?");

    return Ok(Json(ret));
}

#[get("/clientes/<id>/extrato")]
async fn pega_extrato(pool: &rocket::State<PgPool>, id: i32) -> Result<Json<Extrato>, Status> {
    if id < 1 || id > 5 {
        return Err(Status::NotFound);
    }

    let futuro_saldo = sqlx::query_as!(Saldo, "SELECT saldo, limite FROM users WHERE id = $1", id)
        .fetch_one(&**pool);

    let transações: Vec<Transação> = sqlx::query_as!(
        Transação,
        r#"SELECT valor, tipo as "tipo: TipoTransação", descricao, realizada_em from ledger
	    WHERE id_cliente = $1 ORDER BY realizada_em DESC LIMIT 10"#,
        id
    )
    .fetch_all(&**pool)
    .await
    .expect("Não consegui pegar o extrato");

    let saldo = futuro_saldo.await.expect("Sem saldo");

    let ret = Extrato {
        total: saldo.saldo,
        data_extrato: Utc::now(),
        limite: saldo.limite,
        ultimas_transacoes: transações,
    };
    return Ok(Json(ret));
}

#[launch]
async fn rocket() -> _ {
    let args = Args::parse();
    let pool = sqlx::postgres::PgPoolOptions::new()
	.max_connections(128)
	.min_connections(32)
	.connect(args.dburi.as_str())
	.await
        .expect("Não rolou de conectar ao banco");

    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("Não rolou de popular o banco");

    rocket::build()
        .configure(rocket::Config::figment().merge(("port", 9999)))
        .register("/", catchers![not_found, unprocessable, default_catcher])
        .manage::<PgPool>(pool)
        .mount("/", routes![põe_transação, pega_extrato])
}
