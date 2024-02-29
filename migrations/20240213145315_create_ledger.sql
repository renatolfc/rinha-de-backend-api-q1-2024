CREATE TYPE tipot AS ENUM ('C', 'D');
CREATE TABLE ledger (
  id SERIAL PRIMARY KEY,
  id_cliente INTEGER NOT NULL,
  valor INTEGER NOT NULL,
  tipo tipot NOT NULL,
  descricao VARCHAR(10) NOT NULL,
  realizada_em TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY (id_cliente) REFERENCES users(id)
);

CREATE INDEX realizada_idx ON ledger(realizada_em DESC, id_cliente);
