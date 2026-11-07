use chrono::NaiveDate;
use rust_decimal::Decimal;

use super::catalogo::{Benchmark, Indexador, Ir, Liquidez, TipoInfo};

/// Entrada como chega da interface, ainda não validada.
#[derive(Debug, Clone, Default)]
pub struct NovoInvestimento {
    pub tipo: String,
    pub instituicao: String,
    pub emissor: Option<String>,
    pub ticker: Option<String>,
    pub indexador: Option<String>,
    pub taxa: Option<String>,
    pub benchmark: Option<String>,
    pub data_aplicacao: Option<String>,
    pub vencimento: Option<String>,
    pub valor_aplicado_centavos: Option<i64>,
    pub quantidade: Option<i64>,
    pub preco_medio_centavos: Option<i64>,
    pub liquidez: Option<String>,
    pub data_carencia: Option<String>,
    pub ir: Option<String>,
    pub observacao: Option<String>,
    /// CNPJ do fundo (só para o tipo `fundo`), com ou sem máscara.
    pub cnpj: Option<String>,
    pub meta_id: Option<i64>,
}

/// `campo` é o nome camelCase do campo de entrada, para a interface marcá-lo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErroCampo {
    pub campo: String,
    pub mensagem: String,
}

/// Investimento já convertido; campos fora da forma do tipo ficam `None`.
#[derive(Debug, Clone)]
pub struct InvestimentoValido {
    pub tipo: &'static TipoInfo,
    pub instituicao: String,
    pub emissor: Option<String>,
    pub ticker: Option<String>,
    pub indexador: Option<Indexador>,
    pub taxa: Option<Decimal>,
    pub benchmark: Option<Benchmark>,
    pub data_aplicacao: NaiveDate,
    pub vencimento: Option<NaiveDate>,
    pub valor_aplicado_centavos: Option<i64>,
    pub quantidade: Option<i64>,
    pub preco_medio_centavos: Option<i64>,
    pub liquidez: Option<Liquidez>,
    pub data_carencia: Option<NaiveDate>,
    pub ir: Option<Ir>,
    pub observacao: Option<String>,
    /// CNPJ do fundo com 14 dígitos (só para o tipo `fundo`).
    pub cnpj: Option<String>,
}
