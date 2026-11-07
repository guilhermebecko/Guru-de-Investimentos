//! Tipos que atravessam a ponte e conversões de/para o núcleo.

use guru_core::{
    Benchmark, Classe, ErroCampo, Garantia, INSTITUICOES, Indexador, Investimento, Ir, Liquidez,
    NovoInvestimento, Resumo, TipoInfo, tipos,
};
use napi_derive::napi;

#[napi(object)]
pub struct Opcao {
    pub codigo: String,
    pub rotulo: String,
}

#[napi(object)]
pub struct TipoCatalogo {
    pub codigo: String,
    pub rotulo: String,
    pub grupo: String,
    pub classe: String,
    pub forma: String,
    pub indexador_forcado: Option<String>,
    pub liquidez_forcada: Option<String>,
    pub ir_padrao: Option<String>,
    pub ir_editavel: bool,
    pub garantia: Option<String>,
    pub exige_emissor: bool,
    pub liquidez_padrao: Option<String>,
}

#[napi(object)]
pub struct Catalogo {
    pub tipos: Vec<TipoCatalogo>,
    pub classes: Vec<Opcao>,
    pub indexadores: Vec<Opcao>,
    pub benchmarks: Vec<Opcao>,
    pub liquidezes: Vec<Opcao>,
    pub irs: Vec<Opcao>,
    pub garantias: Vec<Opcao>,
    pub instituicoes: Vec<String>,
}

#[napi(object)]
pub struct NovoInvestimentoJs {
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
    pub cnpj: Option<String>,
    pub meta_id: Option<i64>,
}

#[napi(object)]
pub struct ErroCampoJs {
    pub campo: String,
    pub mensagem: String,
}

#[napi(object)]
pub struct ResultadoGravacao {
    pub id: Option<i64>,
    pub erros: Vec<ErroCampoJs>,
}

#[napi(object)]
pub struct InvestimentoJs {
    pub id: i64,
    pub tipo: String,
    pub tipo_rotulo: String,
    pub classe: String,
    pub classe_rotulo: String,
    pub nome: String,
    pub instituicao: String,
    pub emissor: Option<String>,
    pub ticker: Option<String>,
    pub indexador: Option<String>,
    pub indexador_rotulo: Option<String>,
    pub taxa: Option<String>,
    pub benchmark: Option<String>,
    pub rendimento: Option<String>,
    pub data_aplicacao: String,
    pub vencimento: Option<String>,
    pub valor_aplicado_centavos: i64,
    pub quantidade: Option<i64>,
    pub preco_medio_centavos: Option<i64>,
    pub liquidez: Option<String>,
    pub liquidez_rotulo: Option<String>,
    pub data_carencia: Option<String>,
    pub ir: Option<String>,
    pub ir_rotulo: Option<String>,
    pub garantia: Option<String>,
    pub garantia_rotulo: Option<String>,
    pub observacao: Option<String>,
    /// CNPJ do fundo formatado; `null` se o tipo não é fundo ou o código não é um CNPJ válido.
    pub cnpj: Option<String>,
    pub meta_id: Option<i64>,
    pub meta_nome: Option<String>,
    /// Valor bruto (antes do IR e de taxas).
    pub valor_atual_centavos: i64,
    pub rentabilidade_decimos: i64,
    pub origem_valor: String,
    pub origem_valor_rotulo: String,
    pub data_valor: Option<String>,
}

#[napi(object)]
pub struct ResumoClasseJs {
    pub classe: String,
    pub rotulo: String,
    pub total_aplicado_centavos: i64,
    pub total_atual_centavos: i64,
    pub quantidade: i64,
    pub percentual_decimos: i64,
    pub alvo_percentual: Option<i64>,
}

#[napi(object)]
pub struct ResumoJs {
    pub total_aplicado_centavos: i64,
    pub total_atual_centavos: i64,
    pub classes: Vec<ResumoClasseJs>,
}

fn opcoes<T: Copy>(
    todos: &[T],
    codigo: fn(&T) -> &'static str,
    rotulo: fn(&T) -> &'static str,
) -> Vec<Opcao> {
    todos
        .iter()
        .map(|v| Opcao {
            codigo: codigo(v).to_string(),
            rotulo: rotulo(v).to_string(),
        })
        .collect()
}

macro_rules! opcoes {
    ($tipo:ty) => {
        opcoes(<$tipo>::TODOS, <$tipo>::codigo, <$tipo>::rotulo)
    };
}

fn tipo_catalogo(t: &TipoInfo) -> TipoCatalogo {
    TipoCatalogo {
        codigo: t.codigo.to_string(),
        rotulo: t.rotulo.to_string(),
        grupo: t.grupo.to_string(),
        classe: t.classe.codigo().to_string(),
        forma: t.forma.codigo().to_string(),
        indexador_forcado: t.indexador_forcado.map(|v| v.codigo().to_string()),
        liquidez_forcada: t.liquidez_forcada.map(|v| v.codigo().to_string()),
        ir_padrao: t.ir_padrao.map(|v| v.codigo().to_string()),
        ir_editavel: t.ir_editavel,
        garantia: t.garantia.map(|v| v.codigo().to_string()),
        exige_emissor: t.exige_emissor,
        liquidez_padrao: t.liquidez_padrao.map(|v| v.codigo().to_string()),
    }
}

pub fn catalogo() -> Catalogo {
    Catalogo {
        tipos: tipos().iter().map(tipo_catalogo).collect(),
        classes: opcoes!(Classe),
        indexadores: opcoes!(Indexador),
        benchmarks: opcoes!(Benchmark),
        liquidezes: opcoes!(Liquidez),
        irs: opcoes!(Ir),
        garantias: opcoes!(Garantia),
        instituicoes: INSTITUICOES.iter().map(|s| s.to_string()).collect(),
    }
}

pub fn entrada(j: NovoInvestimentoJs) -> NovoInvestimento {
    NovoInvestimento {
        tipo: j.tipo,
        instituicao: j.instituicao,
        emissor: j.emissor,
        ticker: j.ticker,
        indexador: j.indexador,
        taxa: j.taxa,
        benchmark: j.benchmark,
        data_aplicacao: j.data_aplicacao,
        vencimento: j.vencimento,
        valor_aplicado_centavos: j.valor_aplicado_centavos,
        quantidade: j.quantidade,
        preco_medio_centavos: j.preco_medio_centavos,
        liquidez: j.liquidez,
        data_carencia: j.data_carencia,
        ir: j.ir,
        observacao: j.observacao,
        cnpj: j.cnpj,
        meta_id: j.meta_id,
    }
}

pub fn erros(lista: Vec<ErroCampo>) -> Vec<ErroCampoJs> {
    lista
        .into_iter()
        .map(|e| ErroCampoJs {
            campo: e.campo,
            mensagem: e.mensagem,
        })
        .collect()
}

pub fn investimento(i: Investimento) -> InvestimentoJs {
    let cnpj = (i.tipo == "fundo")
        .then(|| i.codigo_b3.as_deref().and_then(guru_core::normalizar_cnpj))
        .flatten()
        .map(|c| guru_core::formatar_cnpj(&c));
    InvestimentoJs {
        id: i.id,
        tipo: i.tipo,
        tipo_rotulo: i.tipo_rotulo,
        classe: i.classe.codigo().to_string(),
        classe_rotulo: i.classe.rotulo().to_string(),
        nome: i.nome,
        instituicao: i.instituicao,
        emissor: i.emissor,
        ticker: i.ticker,
        indexador: i.indexador.map(|v| v.codigo().to_string()),
        indexador_rotulo: i.indexador.map(|v| v.rotulo().to_string()),
        taxa: i.taxa,
        benchmark: i.benchmark.map(|v| v.codigo().to_string()),
        rendimento: i.rendimento,
        data_aplicacao: i.data_aplicacao,
        vencimento: i.vencimento,
        valor_aplicado_centavos: i.valor_aplicado_centavos,
        quantidade: i.quantidade,
        preco_medio_centavos: i.preco_medio_centavos,
        liquidez: i.liquidez.map(|v| v.codigo().to_string()),
        liquidez_rotulo: i.liquidez.map(|v| v.rotulo().to_string()),
        data_carencia: i.data_carencia,
        ir: i.ir.map(|v| v.codigo().to_string()),
        ir_rotulo: i.ir.map(|v| v.rotulo().to_string()),
        garantia: i.garantia.map(|v| v.codigo().to_string()),
        garantia_rotulo: i.garantia.map(|v| v.rotulo().to_string()),
        observacao: i.observacao,
        cnpj,
        meta_id: i.meta_id,
        meta_nome: i.meta_nome,
        valor_atual_centavos: i.valor_atual_centavos,
        rentabilidade_decimos: i.rentabilidade_decimos,
        origem_valor: i.origem_valor.codigo().to_string(),
        origem_valor_rotulo: i.origem_valor.rotulo().to_string(),
        data_valor: i.data_valor.map(|d| d.format("%Y-%m-%d").to_string()),
    }
}

pub fn resumo(r: Resumo) -> ResumoJs {
    ResumoJs {
        total_aplicado_centavos: r.total_aplicado_centavos,
        total_atual_centavos: r.total_atual_centavos,
        classes: r
            .classes
            .into_iter()
            .map(|c| ResumoClasseJs {
                classe: c.classe.codigo().to_string(),
                rotulo: c.classe.rotulo().to_string(),
                total_aplicado_centavos: c.total_aplicado_centavos,
                total_atual_centavos: c.total_atual_centavos,
                quantidade: c.quantidade,
                percentual_decimos: c.percentual_decimos,
                alvo_percentual: c.alvo_percentual,
            })
            .collect(),
    }
}
