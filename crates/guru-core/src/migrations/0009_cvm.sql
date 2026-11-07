CREATE TABLE cota_fundo (
    cnpj TEXT NOT NULL,
    data TEXT NOT NULL,
    cota TEXT NOT NULL,
    PRIMARY KEY (cnpj, data)
) WITHOUT ROWID;
