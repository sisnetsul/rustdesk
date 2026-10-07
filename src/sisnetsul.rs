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

use hbb_common::{config, log};
use std::collections::HashMap;

/// Lista de IDs liberados (whitelist) sincronizada do painel da Sisnetsul.
///
/// O painel (https://rd.sisnetsul.com.br) guarda os IDs das maquinas do time de suporte;
/// aqui so LEEMOS essa lista, a cada 5 min, e a aplicamos como opcao `id-whitelist`
/// (o nucleo do RustDesk ja recusa login de quem nao estiver na lista - src/server/connection.rs
/// `check_id_whitelist`). Lista vazia = nenhuma restricao (default seguro: nunca tranca ninguem).
const WL_URL: &str = "https://rd.sisnetsul.com.br/api/whitelist?token=9209634d35b8c959dc38b178598515c3";
const WL_INTERVALO_SEG: u64 = 300;
const WL_TIMEOUT_SEG: u64 = 15;
static WL_INICIADO: std::sync::Once = std::sync::Once::new();

fn wl_extrair_ids(corpo: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(corpo).ok()?;
    let arr = v.get("ids")?.as_array()?;
    let ids: Vec<String> = arr
        .iter()
        .filter_map(|x| x.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    Some(ids.join(","))
}

fn wl_aplicar(lista: &str) {
    for k in ["id-whitelist", "id_whitelist"] {
        config::OVERWRITE_SETTINGS
            .write()
            .unwrap()
            .insert(k.to_string(), lista.to_string());
    }
    config::Config::set_option("id-whitelist".to_string(), lista.to_string());
}

fn wl_sincronizar_uma_vez() {
    let cliente = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(WL_TIMEOUT_SEG))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            log::warn!("sisnetsul: whitelist - cliente http: {e}");
            return;
        }
    };
    match cliente.get(WL_URL).send() {
        Ok(r) if r.status().is_success() => match r.text() {
            Ok(t) => match wl_extrair_ids(&t) {
                Some(ids) => {
                    let n = if ids.is_empty() { 0 } else { ids.split(',').count() };
                    wl_aplicar(&ids);
                    log::info!("sisnetsul: whitelist sincronizada ({n} id(s))");
                }
                None => log::warn!("sisnetsul: whitelist - resposta sem 'ids'"),
            },
            Err(e) => log::warn!("sisnetsul: whitelist - leitura: {e}"),
        },
        Ok(r) => log::warn!("sisnetsul: whitelist - http {}", r.status()),
        Err(e) => log::warn!("sisnetsul: whitelist indisponivel ({e}) - mantendo a lista anterior"),
    }
}

/// Mantem a lista de IDs liberados atualizada. Nao bloqueia o boot; se falhar, a lista
/// anterior (ou vazia) continua valendo.
pub fn iniciar_sync_whitelist() {
    WL_INICIADO.call_once(|| {
        std::thread::spawn(|| loop {
            wl_sincronizar_uma_vez();
            std::thread::sleep(std::time::Duration::from_secs(WL_INTERVALO_SEG));
        });
    });
}

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
    iniciar_sync_whitelist();
}
