//! Sisnetsul — defaults e travas do cliente entregue ao cliente final.
//!
//! O binario entregue ao cliente da Sisnetsul:
//!   * so aponta para o NOSSO servidor (rendezvous/key/api/relay fixos no pacote);
//!   * tem as configuracoes de servidor/rede/seguranca escondidas;
//!   * nao troca ID nem senha permanente;
//!   * e **RECEIVE-ONLY**: so RECEBE atendimento, nunca inicia/controla outra maquina.
//!
//! O build interno da equipe (suporte) usa o mesmo codigo com `RECEIVE_ONLY = false`
//! em outra branch/const.

use hbb_common::config;
use std::collections::HashMap;

/// `true` = cliente "receber atendimento apenas" (bloqueia o cliente de iniciar conexao).
pub const RECEIVE_ONLY: bool = true;

const DEFAULTS: &str = include_str!("../sisnetsul-defaults.json");

/// Trava de "so recebe": usada no lugar de `config::is_incoming_only()`.
pub fn receive_only() -> bool {
    RECEIVE_ONLY
}

/// Aplica os defaults/travas Sisnetsul por cima da config (idempotente).
pub fn apply_defaults() {
    let map: HashMap<String, String> = match serde_json::from_str(DEFAULTS) {
        Ok(m) => m,
        Err(e) => {
            log::error!("sisnetsul: defaults invalidos: {e}");
            return;
        }
    };
    let mut aplicados = 0usize;
    for (k, v) in map {
        if v.trim().is_empty() || k.starts_with('_') {
            continue;
        }
        let k_alt = k.replace('-', "_");
        {
            let mut hard = config::HARD_SETTINGS.write().unwrap();
            hard.insert(k.clone(), v.clone());
            hard.insert(k_alt.clone(), v.clone());
        }
        {
            let mut local = config::OVERWRITE_LOCAL_SETTINGS.write().unwrap();
            local.insert(k.clone(), v.clone());
            local.insert(k_alt.clone(), v.clone());
        }
        {
            let mut srv = config::OVERWRITE_SETTINGS.write().unwrap();
            srv.insert(k.clone(), v.clone());
            srv.insert(k_alt.clone(), v.clone());
        }
        {
            let mut disp = config::OVERWRITE_DISPLAY_SETTINGS.write().unwrap();
            disp.insert(k.clone(), v.clone());
            disp.insert(k_alt.clone(), v.clone());
        }
        aplicados += 1;
    }
    log::info!("sisnetsul: {aplicados} defaults aplicados (receive_only={RECEIVE_ONLY})");
}
