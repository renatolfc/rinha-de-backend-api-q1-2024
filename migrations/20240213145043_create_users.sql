CREATE TABLE users (
  id SERIAL PRIMARY KEY,
  limite INTEGER NOT NULL,
  saldo INTEGER NOT NULL
);

INSERT INTO users(limite, saldo)
VALUES
  (100000, 0),
  (80000, 0),
  (1000000, 0),
  (10000000, 0),
  (500000, 0);
