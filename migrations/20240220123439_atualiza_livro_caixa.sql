CREATE PROCEDURE atualiza_livro_caixa(
  id_cliente INTEGER,
  valor INTEGER,
  valor_extrato INTEGER, 
  tipo tipot,
  descricao VARCHAR(10),
  OUT saldo_atual INTEGER,
  OUT limite_atual INTEGER
)
LANGUAGE plpgsql AS
$$
BEGIN
  INSERT INTO ledger (id_cliente, valor, tipo, descricao) VALUES (id_cliente, valor, tipo, descricao);
  UPDATE users
  SET saldo = saldo + valor_extrato
  WHERE id = id_cliente RETURNING saldo, limite INTO saldo_atual, limite_atual;
  COMMIT;
  RETURN;
END;
$$;
