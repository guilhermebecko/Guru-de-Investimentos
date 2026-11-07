//! Leitores puros dos formatos oficiais de dados de mercado (sem rede).

mod atualizador;
mod cotahist;
mod cvm;
mod rede;
mod repositorio;
mod sgs;
mod tesouro;
mod valorizacao;

use std::fmt;

use chrono::NaiveDate;

pub use atualizador::{ResultadoAtualizacao, atualizar, titulo_tesouro};
pub use cotahist::{Cotacao, ler_cotahist_txt, ler_cotahist_zip};
pub use cvm::{CotaFundo, ler_inf_diario_csv, ler_inf_diario_zip};
pub use rede::{Baixador, BaixadorHttp, BaixadorPasta};
pub use repositorio::{B3Alvo, StatusFonte};
pub use sgs::{PontoDiario, ler_sgs};
pub use tesouro::{PrecoTesouro, ler_tesouro};
pub use valorizacao::{Origem, Valor, valorizar};
pub(crate) use valorizacao::{Series, cota_base_serve, valorizar_com, valorizar_datas};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroFonte {
    Rede(String),
    Status(u16),
    Formato(String),
}

impl fmt::Display for ErroFonte {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErroFonte::Rede(m) => write!(f, "Falha de rede: {m}"),
            ErroFonte::Status(c) => write!(f, "A fonte respondeu com erro (status {c})."),
            ErroFonte::Formato(m) => write!(f, "Formato inesperado da fonte: {m}"),
        }
    }
}

impl std::error::Error for ErroFonte {}

/// `dd/MM/aaaa`.
pub(crate) fn data_br(s: &str) -> Result<NaiveDate, ErroFonte> {
    NaiveDate::parse_from_str(s.trim(), "%d/%m/%Y")
        .map_err(|_| ErroFonte::Formato(format!("data inválida: {s}")))
}
