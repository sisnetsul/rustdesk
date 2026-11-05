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

// ---------------------------------------------------------------------------
// Modo SEM ADMINISTRADOR
//
// Muitos clientes nao tem conta de administrador e mesmo assim usam acesso
// remoto (o AnyDesk roda assim). Para funcionar igual, o cliente:
//   1. copia a si mesmo para %LOCALAPPDATA%\Sisnetsul\SisnetAssist\  (sem UAC)
//   2. registra inicio automatico no logon daquele usuario (HKCU ...\Run)
//   3. gera uma senha fixa propria e a reporta ao nosso painel, para que o
//      suporte entre sem precisar que alguem abra a janela.
// Nada disso exige elevacao; o servico (com admin) continua sendo o caminho
// melhor quando existir — este e o caminho possivel quando nao existe.
// ---------------------------------------------------------------------------

const REG_URL: &str = "https://rd.sisnetsul.com.br/api/device-register?token=9209634d35b8c959dc38b178598515c3";
const REG_TIMEOUT_SEG: u64 = 15;

/// Senha forte o suficiente para acesso nao assistido (16 hex = 64 bits),
/// gerada localmente sem dependencia extra.
fn gerar_senha() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut saida = String::new();
    while saida.len() < 16 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        h.write_u128(nanos);
        h.write_u32(std::process::id());
        h.write(&saida.clone().into_bytes());
        saida.push_str(&format!("{:016x}", h.finish()));
    }
    saida.truncate(16);
    saida
}

/// Copia o executavel para a pasta do usuario e cria o inicio automatico no logon.
/// Idempotente e silencioso: nunca derruba o app se falhar.
pub fn preparar_usuario() {
    #[cfg(windows)]
    {
        use std::process::Command;
        let local = match std::env::var("LOCALAPPDATA") {
            Ok(v) if !v.trim().is_empty() => v,
            _ => return,
        };
        let dir = format!("{local}\\Sisnetsul\\SisnetAssist");
        let destino = format!("{dir}\\SisnetAssist.exe");
        let atual = std::env::current_exe()
            .ok()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        if atual.is_empty() {
            return;
        }
        if !atual.eq_ignore_ascii_case(&destino) {
            let _ = std::fs::create_dir_all(&dir);
            if let Ok(bytes) = std::fs::read(&atual) {
                if !std::path::Path::new(&destino).exists()
                    || std::fs::metadata(&destino).map(|m| m.len()).unwrap_or(0) != bytes.len() as u64
                {
                    let _ = std::fs::write(&destino, bytes);
                }
            }
        }
        let alvo = if std::path::Path::new(&destino).exists() { destino } else { atual };
        let _ = Command::new("reg")
            .args([
                "add",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "SisnetAssist",
                "/t",
                "REG_SZ",
                "/d",
                &format!("\"{alvo}\""),
                "/f",
            ])
            .output();
        log::info!("sisnetsul: inicio automatico no logon -> {alvo}");
    }
}

/// Reporta id + senha ao painel. So grava a senha localmente DEPOIS que o painel
/// confirmou (se a rede falhar, tenta de novo no proximo start com outra senha).
fn registrar_uma_vez() -> bool {
    let id = config::Config::get_id();
    if id.trim().is_empty() {
        return false;
    }
    if config::Config::has_permanent_password() {
        return true; // ja tem senha (nossa ou definida antes): nao mexe
    }
    let senha = gerar_senha();
    let cliente = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(REG_TIMEOUT_SEG))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            log::warn!("sisnetsul: registro - cliente http: {e}");
            return false;
        }
    };
    let corpo = serde_json::json!({
        "id": id,
        "senha": senha,
        "hostname": whoami_hostname(),
    });
    match cliente.post(REG_URL).json(&corpo).send() {
        Ok(r) if r.status().is_success() => {
            config::Config::set_permanent_password(&senha);
            log::info!("sisnetsul: dispositivo registrado no painel ({id})");
            true
        }
        Ok(r) => {
            log::warn!("sisnetsul: registro - http {}", r.status());
            false
        }
        Err(e) => {
            log::warn!("sisnetsul: painel indisponivel para registro ({e})");
            false
        }
    }
}

fn whoami_hostname() -> String {
    #[cfg(windows)]
    {
        if let Ok(v) = std::env::var("COMPUTERNAME") {
            return v;
        }
    }
    #[cfg(not(windows))]
    {
        if let Ok(v) = std::env::var("HOSTNAME") {
            return v;
        }
    }
    String::new()
}

/// Em thread: garante instalacao no perfil do usuario e o registro do dispositivo.
/// Nao bloqueia o boot e nao incomoda o usuario final.
pub fn iniciar_modo_usuario() {
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(15));
        preparar_usuario();
        for tentativa in 1..=6u32 {
            if registrar_uma_vez() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_secs(60 * tentativa as u64));
        }
        log::warn!("sisnetsul: sem registro de dispositivo nesta execucao");
    });
}

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
    if RECEIVE_ONLY {
        static MODO_USUARIO: std::sync::Once = std::sync::Once::new();
        MODO_USUARIO.call_once(iniciar_modo_usuario);
    }
}
