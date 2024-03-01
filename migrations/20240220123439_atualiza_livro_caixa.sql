CREATE PROCEDURE poe(
  idc INTEGER,
  v INTEGER,
  d VARCHAR(10),
  INOUT saldo_atual INTEGER = NULL,
  INOUT limite_atual INTEGER = NULL
)
LANGUAGE plpgsql AS
$$
BEGIN
  PERFORM pg_advisory_xact_lock(idc);

  INSERT INTO ledger (
    id_cliente,
    valor,
    tipo,
    descricao
  ) VALUES (idc, v, 'C', d);

  UPDATE users
  SET saldo = saldo + v
    WHERE users.id = idc
    RETURNING saldo, limite INTO saldo_atual, limite_atual;
  COMMIT;
END;
$$;

CREATE PROCEDURE tira(
  idc INTEGER,
  v INTEGER,
  d VARCHAR(10),
  INOUT saldo_atual INTEGER = NULL,
  INOUT limite_atual INTEGER = NULL
)
LANGUAGE plpgsql AS
$$
BEGIN
  PERFORM pg_advisory_xact_lock(idc);

  SELECT limite, saldo INTO limite_atual, saldo_atual
  FROM users
  WHERE id = idc;

  IF saldo_atual - v >= limite_atual * -1 THEN
    INSERT INTO ledger (
      id_cliente,
      valor,
      tipo,
      descricao
    ) VALUES (idc, v, 'D', d);

    UPDATE users
    SET saldo = saldo - v
      WHERE users.id = idc
      RETURNING saldo, limite INTO saldo_atual, limite_atual;
    COMMIT;
  ELSE
    SELECT -1, -1 INTO saldo_atual, limite_atual;
  END IF;
END;
$$;
