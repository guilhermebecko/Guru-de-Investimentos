//! CNPJ de fundo: normalização com verificação dos dígitos e formatação.

fn digito(digitos: &[u32], pesos: &[u32]) -> u32 {
    let soma: u32 = digitos.iter().zip(pesos).map(|(d, p)| d * p).sum();
    let resto = soma % 11;
    if resto < 2 { 0 } else { 11 - resto }
}

/// Tira tudo que não é dígito; `Some` só com 14 dígitos de DV válido e não todos iguais.
pub fn normalizar_cnpj(texto: &str) -> Option<String> {
    let digitos: Vec<u32> = texto.chars().filter_map(|c| c.to_digit(10)).collect();
    if digitos.len() != 14 || digitos.iter().all(|&d| d == digitos[0]) {
        return None;
    }
    const P1: [u32; 12] = [5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2];
    const P2: [u32; 13] = [6, 5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2];
    if digito(&digitos[..12], &P1) != digitos[12] || digito(&digitos[..13], &P2) != digitos[13] {
        return None;
    }
    Some(
        digitos
            .iter()
            .filter_map(|&d| char::from_digit(d, 10))
            .collect(),
    )
}

/// `00.000.000/0000-00` a partir dos 14 dígitos; outra coisa volta sem mudança.
pub fn formatar_cnpj(digitos: &str) -> String {
    if digitos.len() != 14 || !digitos.bytes().all(|b| b.is_ascii_digit()) {
        return digitos.to_string();
    }
    format!(
        "{}.{}.{}/{}-{}",
        &digitos[0..2],
        &digitos[2..5],
        &digitos[5..8],
        &digitos[8..12],
        &digitos[12..14]
    )
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn normaliza_cnpj_valido() {
        assert_eq!(
            normalizar_cnpj("11.222.333/0001-81"),
            Some("11222333000181".to_string())
        );
        assert_eq!(
            normalizar_cnpj(" 11222333000181 "),
            Some("11222333000181".to_string())
        );
    }

    #[test]
    fn recusa_cnpj_invalido() {
        assert_eq!(normalizar_cnpj("11.222.333/0001-82"), None);
        assert_eq!(normalizar_cnpj("11.222.333/0001-8"), None);
        assert_eq!(normalizar_cnpj("00000000000000"), None);
        assert_eq!(normalizar_cnpj(""), None);
    }

    #[test]
    fn formata_cnpj() {
        assert_eq!(formatar_cnpj("11222333000181"), "11.222.333/0001-81");
    }
}
