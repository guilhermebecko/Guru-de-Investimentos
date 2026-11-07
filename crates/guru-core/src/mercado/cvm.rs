//! Leitor do Informe Diário de fundos da CVM (cota diária por CNPJ).

use std::collections::HashSet;
use std::io::Read;
use std::str::FromStr;

use chrono::NaiveDate;
use rust_decimal::Decimal;

use super::ErroFonte;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CotaFundo {
    /// 14 dígitos.
    pub cnpj: String,
    pub data: NaiveDate,
    pub cota: Decimal,
}

/// Lê o CSV já decodificado. Só linhas de `cnpjs` (14 dígitos) sem subclasse; o par
/// `(cnpj, data)` repetido fica com a primeira linha.
pub fn ler_inf_diario_csv(
    texto: &str,
    cnpjs: &HashSet<String>,
) -> Result<Vec<CotaFundo>, ErroFonte> {
    let mut linhas = texto.trim_start_matches('\u{feff}').lines();
    let cabecalho: Vec<&str> = linhas
        .next()
        .ok_or_else(|| ErroFonte::Formato("Informe Diário vazio".into()))?
        .split(';')
        .map(str::trim)
        .collect();
    let achar = |nome: &str| cabecalho.iter().position(|c| *c == nome);
    let col_cnpj = achar("CNPJ_FUNDO_CLASSE").or_else(|| achar("CNPJ_FUNDO"));
    let (Some(col_cnpj), Some(col_data), Some(col_cota)) =
        (col_cnpj, achar("DT_COMPTC"), achar("VL_QUOTA"))
    else {
        return Err(ErroFonte::Formato(
            "Informe Diário sem as colunas esperadas".into(),
        ));
    };
    let col_sub = achar("ID_SUBCLASSE");

    let mut vistos: HashSet<(String, NaiveDate)> = HashSet::new();
    let mut saida = Vec::new();
    for linha in linhas {
        let campos: Vec<&str> = linha.split(';').collect();
        let pega = |i: usize| campos.get(i).map(|c| c.trim());
        if col_sub.and_then(pega).is_some_and(|s| !s.is_empty()) {
            continue;
        }
        let Some(cnpj) =
            pega(col_cnpj).map(|c| c.chars().filter(char::is_ascii_digit).collect::<String>())
        else {
            continue;
        };
        if !cnpjs.contains(&cnpj) {
            continue;
        }
        let Some(data) = pega(col_data).and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        else {
            continue;
        };
        let Some(cota) = pega(col_cota).and_then(|c| Decimal::from_str(c).ok()) else {
            continue;
        };
        if cota <= Decimal::ZERO {
            continue;
        }
        if vistos.insert((cnpj.clone(), data)) {
            saida.push(CotaFundo { cnpj, data, cota });
        }
    }
    Ok(saida)
}

/// Lê o primeiro `.csv` do zip; UTF-8 ou, se inválido, latin-1.
pub fn ler_inf_diario_zip(
    bytes: &[u8],
    cnpjs: &HashSet<String>,
) -> Result<Vec<CotaFundo>, ErroFonte> {
    let invalido = || ErroFonte::Formato("arquivo da CVM não é um ZIP válido".into());
    let mut arquivo = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|_| invalido())?;
    for i in 0..arquivo.len() {
        let mut entrada = arquivo.by_index(i).map_err(|_| invalido())?;
        if entrada
            .name()
            .map_err(|_| invalido())?
            .to_ascii_lowercase()
            .ends_with(".csv")
        {
            let mut conteudo = Vec::new();
            entrada.read_to_end(&mut conteudo).map_err(|_| invalido())?;
            let texto = match String::from_utf8(conteudo) {
                Ok(t) => t,
                Err(e) => e.into_bytes().iter().map(|&b| b as char).collect(),
            };
            return ler_inf_diario_csv(&texto, cnpjs);
        }
    }
    Err(ErroFonte::Formato("ZIP da CVM sem arquivo .csv".into()))
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    const NOVO: &str = "TP_FUNDO_CLASSE;CNPJ_FUNDO_CLASSE;ID_SUBCLASSE;DT_COMPTC;VL_TOTAL;VL_QUOTA;VL_PATRIM_LIQ;CAPTC_DIA;RESG_DIA;NR_COTST\r\n";
    const ANTIGO: &str = "TP_FUNDO;CNPJ_FUNDO;DT_COMPTC;VL_TOTAL;VL_QUOTA;VL_PATRIM_LIQ;CAPTC_DIA;RESG_DIA;NR_COTST\r\n";

    fn filtro(cnpjs: &[&str]) -> HashSet<String> {
        cnpjs.iter().map(|c| c.to_string()).collect()
    }

    fn cota(cnpj: &str, data: (i32, u32, u32), v: &str) -> CotaFundo {
        CotaFundo {
            cnpj: cnpj.into(),
            data: NaiveDate::from_ymd_opt(data.0, data.1, data.2).unwrap(),
            cota: Decimal::from_str(v).unwrap(),
        }
    }

    #[test]
    fn le_layout_novo_e_antigo() {
        let f = filtro(&["11222333000181"]);
        let novo = format!(
            "{NOVO}CLASSES - FIF;11.222.333/0001-81;;2026-11-05;100.00;1.498960195115;90.00;0;0;3\r\n"
        );
        let antigo = format!(
            "{ANTIGO}FI;11.222.333/0001-81;2026-11-05;100.00;1.498960195115;90.00;0;0;3\r\n"
        );
        let esperado = vec![cota("11222333000181", (2026, 11, 5), "1.498960195115")];
        assert_eq!(ler_inf_diario_csv(&novo, &f).unwrap(), esperado);
        assert_eq!(ler_inf_diario_csv(&antigo, &f).unwrap(), esperado);
    }

    #[test]
    fn ignora_subclasse_e_cnpj_fora_do_filtro() {
        let csv = format!(
            "{NOVO}X;11.222.333/0001-81;ABC1;2026-11-05;1;2.5;1;0;0;1\r\n\
             X;11.222.333/0001-81;;2026-11-06;1;1.5;1;0;0;1\r\n\
             X;99.999.999/0001-99;;2026-11-06;1;3.5;1;0;0;1\r\n\
             X;55.666.777/0001-00;ABC1;2026-11-06;1;4.5;1;0;0;1\r\n"
        );
        let r = ler_inf_diario_csv(&csv, &filtro(&["11222333000181", "55666777000100"])).unwrap();
        assert_eq!(r, vec![cota("11222333000181", (2026, 11, 6), "1.5")]);
        let so_sub = ler_inf_diario_csv(&csv, &filtro(&["55666777000100"])).unwrap();
        assert!(so_sub.is_empty());
    }

    #[test]
    fn recusa_csv_sem_colunas() {
        let f = filtro(&["11222333000181"]);
        let sem_cota = "TP_FUNDO;CNPJ_FUNDO;DT_COMPTC\r\nFI;11.222.333/0001-81;2026-11-05\r\n";
        assert!(matches!(
            ler_inf_diario_csv(sem_cota, &f),
            Err(ErroFonte::Formato(_))
        ));
        assert!(matches!(
            ler_inf_diario_csv("", &f),
            Err(ErroFonte::Formato(_))
        ));
        let csv = format!(
            "{ANTIGO}FI;11.222.333/0001-81;2026-11-01;1;0;1;0;0;1\r\n\
             FI;11.222.333/0001-81;2026-11-02;1;-1.0;1;0;0;1\r\n\
             FI;11.222.333/0001-81;2026-11-03;1;abc;1;0;0;1\r\n\
             FI;11.222.333/0001-81;2026-11-04;1;1.1;1;0;0;1\r\n\
             FI;11.222.333/0001-81;2026-11-04;1;9.9;1;0;0;1\r\n"
        );
        assert_eq!(
            ler_inf_diario_csv(&csv, &f).unwrap(),
            vec![cota("11222333000181", (2026, 11, 4), "1.1")]
        );
    }

    fn zip_de(nome: &str, conteudo: &[u8]) -> Vec<u8> {
        let mut z = ZipWriter::new(Cursor::new(Vec::new()));
        z.start_file(nome, SimpleFileOptions::default()).unwrap();
        z.write_all(conteudo).unwrap();
        z.finish().unwrap().into_inner()
    }

    #[test]
    fn le_do_zip() {
        let f = filtro(&["11222333000181"]);
        let csv = format!("{ANTIGO}FI;11.222.333/0001-81;2026-11-05;1;1.25;1;0;0;1\r\n");
        let zip = zip_de("inf_diario_fi_202611.csv", csv.as_bytes());
        assert_eq!(
            ler_inf_diario_zip(&zip, &f).unwrap(),
            vec![cota("11222333000181", (2026, 11, 5), "1.25")]
        );
        // latin-1: byte 0xE7 (ç) não é UTF-8 válido
        let mut latin = ANTIGO.as_bytes().to_vec();
        latin.extend_from_slice(b"FI\xE7;11.222.333/0001-81;2026-11-05;1;1.25;1;0;0;1\r\n");
        let zip = zip_de("a.CSV", &latin);
        assert_eq!(ler_inf_diario_zip(&zip, &f).unwrap().len(), 1);
        assert!(matches!(
            ler_inf_diario_zip(b"nao e zip", &f),
            Err(ErroFonte::Formato(_))
        ));
        let sem_csv = zip_de("a.txt", b"x");
        assert!(matches!(
            ler_inf_diario_zip(&sem_csv, &f),
            Err(ErroFonte::Formato(_))
        ));
    }
}
