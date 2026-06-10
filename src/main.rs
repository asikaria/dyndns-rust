use clap::Parser;
use serde::{Deserialize, Serialize};

#[derive(Parser, Debug)]
#[command(author, version, about = "A lightweight Cloudflare DDNS client", long_about = None)]
struct Args {
    #[arg(short, long, help = "Path to the configuration TOML file")]
    config: Option<String>,

    #[arg(short, long, action = clap::ArgAction::SetTrue, help = "Force update even if IP matches cache")]
    force: bool,

    #[arg(short, long, action = clap::ArgAction::SetTrue, help = "Simulate the run without making modifications")]
    dry_run: bool,
}

#[derive(Deserialize, Debug)]
struct Config {
    zone_id: String,
    record_name: String,
    proxied: Option<bool>,
    ttl: Option<u32>,
    cache_file: String,
    endpoints: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct CloudflareError {
    code: i32,
    message: String,
}

#[derive(Deserialize, Debug)]
struct CloudflareErrorBody {
    errors: Vec<CloudflareError>,
}

#[derive(Deserialize, Debug)]
struct DnsRecord {
    id: String,
    #[serde(rename = "type")]
    record_type: String,
    name: String,
    content: String,
    proxied: bool,
    ttl: u32,
}

#[derive(Deserialize, Debug)]
struct DnsRecordsResponse {
    result: Vec<DnsRecord>,
    success: bool,
    errors: Vec<CloudflareError>,
}

#[derive(Serialize, Debug)]
struct UpdateDnsRecordPayload<'a> {
    #[serde(rename = "type")]
    record_type: &'static str,
    name: &'a str,
    content: &'a str,
    ttl: u32,
    proxied: bool,
}

#[derive(Deserialize, Debug)]
struct UpdateResponse {
    success: bool,
    errors: Vec<CloudflareError>,
}

fn load_config(
    path_arg: &Option<String>,
) -> Result<(Config, std::path::PathBuf), Box<dyn std::error::Error>> {
    let path = if let Some(p) = path_arg {
        std::path::PathBuf::from(p)
    } else {
        let candidates = [
            std::path::PathBuf::from("config.toml"),
            std::path::PathBuf::from("/etc/dyndns/config.toml"),
        ];
        match candidates.iter().find(|p| p.exists()) {
            Some(found) => found.clone(),
            None => {
                return Err(
                    format!("No configuration file found. Checked: {:?}", candidates).into(),
                );
            }
        }
    };

    log::info!("Loading configuration from {:?}", path);
    let content = std::fs::read_to_string(&path)?;
    let config: Config = toml::from_str(&content)?;
    Ok((config, path))
}

fn fetch_public_ip(endpoints: &[String]) -> Result<std::net::Ipv4Addr, Box<dyn std::error::Error>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("dyndns-client/0.1")
        .build()?;

    for endpoint in endpoints {
        log::debug!("Attempting to fetch IP from {}", endpoint);
        match client.get(endpoint).send() {
            Ok(response) => {
                if !response.status().is_success() {
                    log::warn!(
                        "Endpoint {} returned status {}",
                        endpoint,
                        response.status()
                    );
                    continue;
                }
                match response.text() {
                    Ok(text) => {
                        let trimmed = text.trim();
                        match trimmed.parse::<std::net::Ipv4Addr>() {
                            Ok(ip) => {
                                log::info!("Successfully retrieved IP: {}", ip);
                                return Ok(ip);
                            }
                            Err(e) => {
                                log::warn!(
                                    "Failed to parse IP '{}' from {}: {}",
                                    trimmed,
                                    endpoint,
                                    e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to read response body from {}: {}", endpoint, e);
                    }
                }
            }
            Err(e) => {
                log::warn!("HTTP request to {} failed: {}", endpoint, e);
            }
        }
    }

    Err("All IP fetching endpoints failed".into())
}

fn read_cached_ip(path: &std::path::Path) -> Option<std::net::Ipv4Addr> {
    if !path.exists() {
        return None;
    }
    match std::fs::read_to_string(path) {
        Ok(content) => {
            let trimmed = content.trim();
            match trimmed.parse::<std::net::Ipv4Addr>() {
                Ok(ip) => Some(ip),
                Err(e) => {
                    log::warn!(
                        "Failed to parse cached IP '{}' in {:?}: {}",
                        trimmed,
                        path,
                        e
                    );
                    None
                }
            }
        }
        Err(e) => {
            log::warn!("Failed to read cache file {:?}: {}", path, e);
            None
        }
    }
}

fn write_cached_ip(
    path: &std::path::Path,
    ip: std::net::Ipv4Addr,
) -> Result<(), Box<dyn std::error::Error>> {
    let parent = path.parent();
    if let Some(p) = parent.filter(|p| !p.exists()) {
        std::fs::create_dir_all(p)?;
    }
    std::fs::write(path, ip.to_string())?;
    Ok(())
}

/// Builds a diagnostic message from a failed Cloudflare HTTP response, preferring
/// the structured `errors` array in the body and falling back to the raw text.
fn describe_cf_error(status: reqwest::StatusCode, body: &str) -> String {
    if let Ok(parsed) = serde_json::from_str::<CloudflareErrorBody>(body)
        && !parsed.errors.is_empty()
    {
        let msgs: Vec<String> = parsed
            .errors
            .iter()
            .map(|e| format!("[{}]: {}", e.code, e.message))
            .collect();
        return format!("status {}: {}", status, msgs.join(", "));
    }
    format!("status {}: {}", status, body.trim())
}

fn get_cloudflare_dns_record(
    client: &reqwest::blocking::Client,
    zone_id: &str,
    record_name: &str,
    api_token: &str,
) -> Result<DnsRecord, Box<dyn std::error::Error>> {
    let url = format!(
        "https://api.cloudflare.com/client/v4/zones/{}/dns_records?name={}",
        zone_id, record_name
    );

    let response = client
        .get(&url)
        .bearer_auth(api_token)
        .header("Content-Type", "application/json")
        .send()?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(format!(
            "Cloudflare API error for GET record: {}",
            describe_cf_error(status, &body)
        )
        .into());
    }

    let resp_body: DnsRecordsResponse = response.json()?;
    if !resp_body.success {
        let err_msgs: Vec<String> = resp_body
            .errors
            .iter()
            .map(|e| format!("[{}]: {}", e.code, e.message))
            .collect();
        return Err(format!("Cloudflare API errors: {}", err_msgs.join(", ")).into());
    }

    let mut matching_records = resp_body
        .result
        .into_iter()
        .filter(|r| r.record_type == "A" && r.name == record_name);

    match (matching_records.next(), matching_records.next()) {
        (Some(r), None) => Ok(r),
        (Some(_), Some(_)) => Err(format!(
            "Multiple A records found for name '{}' in zone '{}'; expected exactly one",
            record_name, zone_id
        )
        .into()),
        (None, _) => Err(format!(
            "No existing A record found for name '{}' in zone '{}'",
            record_name, zone_id
        )
        .into()),
    }
}

#[allow(clippy::too_many_arguments)]
fn update_cloudflare_dns_record(
    client: &reqwest::blocking::Client,
    zone_id: &str,
    record_id: &str,
    record_name: &str,
    api_token: &str,
    new_ip: &str,
    ttl: u32,
    proxied: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let url = format!(
        "https://api.cloudflare.com/client/v4/zones/{}/dns_records/{}",
        zone_id, record_id
    );

    let payload = UpdateDnsRecordPayload {
        record_type: "A",
        name: record_name,
        content: new_ip,
        ttl,
        proxied,
    };

    let response = client
        .patch(&url)
        .bearer_auth(api_token)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(format!(
            "Cloudflare API error for PATCH record: {}",
            describe_cf_error(status, &body)
        )
        .into());
    }

    let resp_body: UpdateResponse = response.json()?;
    if !resp_body.success {
        let err_msgs: Vec<String> = resp_body
            .errors
            .iter()
            .map(|e| format!("[{}]: {}", e.code, e.message))
            .collect();
        return Err(format!(
            "Cloudflare API errors during update: {}",
            err_msgs.join(", ")
        )
        .into());
    }

    Ok(())
}

fn find_api_token_in_env_files(config_path: Option<&std::path::Path>) -> Option<String> {
    let mut paths = Vec::new();
    if let Some(parent) = config_path.and_then(|cp| cp.parent()) {
        paths.push(parent.join(".env"));
    }
    paths.push(std::path::PathBuf::from(".env"));

    for path in paths {
        if path.exists() {
            log::debug!("Checking for token in {:?}", path);
            if let Ok(content) = std::fs::read_to_string(&path) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() || trimmed.starts_with('#') {
                        continue;
                    }
                    if let Some((key, val)) = trimmed.split_once('=') {
                        let key_trimmed = key.trim();
                        if key_trimmed == "CLOUDFLARE_API_TOKEN" {
                            let token = val.trim().trim_matches('"').trim_matches('\'').to_string();
                            if !token.is_empty() {
                                log::info!("Loaded API Token from {:?}", path);
                                return Some(token);
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn expand_tilde(path: &str) -> std::path::PathBuf {
    if let Some((stripped, home)) = path.strip_prefix('~').zip(std::env::var("HOME").ok()) {
        let mut home_path = std::path::PathBuf::from(home);
        if let Some(sub_path) = stripped.strip_prefix('/') {
            home_path.push(sub_path);
        } else if !stripped.is_empty() {
            home_path.push(stripped);
        }
        return home_path;
    }
    std::path::PathBuf::from(path)
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args = Args::parse();

    let (config, config_path) = match load_config(&args.config) {
        Ok(pair) => pair,
        Err(e) => {
            log::error!("Configuration error: {}", e);
            std::process::exit(1);
        }
    };

    let api_token = match find_api_token_in_env_files(Some(&config_path)) {
        Some(t) => t,
        None => {
            log::error!(
                "Cloudflare API Token not found. Configure CLOUDFLARE_API_TOKEN in a .env file next to config.toml or in the current directory."
            );
            std::process::exit(1);
        }
    };

    if config.zone_id.trim().is_empty() {
        log::error!("'zone_id' cannot be empty in configuration.");
        std::process::exit(1);
    }
    if config.record_name.trim().is_empty() {
        log::error!("'record_name' cannot be empty in configuration.");
        std::process::exit(1);
    }
    if config.endpoints.is_empty() {
        log::error!(
            "No IP fetching endpoints configured. Please specify at least one in configuration."
        );
        std::process::exit(1);
    }

    log::info!("Fetching current public IPv4 address...");
    let public_ip = match fetch_public_ip(&config.endpoints) {
        Ok(ip) => ip,
        Err(e) => {
            log::error!("Failed to fetch public IP: {}", e);
            std::process::exit(1);
        }
    };

    let cache_file_path = expand_tilde(&config.cache_file);
    if !args.force && read_cached_ip(&cache_file_path) == Some(public_ip) {
        log::info!(
            "Current IP ({}) matches the cached IP. No update required.",
            public_ip
        );
        std::process::exit(0);
    } else if args.force {
        log::info!("Bypassing cache check (--force active).");
    }

    let http_client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("dyndns-client/0.1")
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to build HTTP Client: {}", e);
            std::process::exit(1);
        }
    };

    log::info!(
        "Retrieving DNS record '{}' from Cloudflare...",
        config.record_name
    );
    let existing_record = match get_cloudflare_dns_record(
        &http_client,
        &config.zone_id,
        &config.record_name,
        &api_token,
    ) {
        Ok(r) => r,
        Err(e) => {
            log::error!("Failed to retrieve DNS record: {}", e);
            std::process::exit(1);
        }
    };

    let existing_ip = match existing_record.content.parse::<std::net::Ipv4Addr>() {
        Ok(ip) => ip,
        Err(_) => {
            log::warn!(
                "Existing DNS record content is not a valid IPv4: {}",
                existing_record.content
            );
            "0.0.0.0".parse::<std::net::Ipv4Addr>().unwrap()
        }
    };

    let new_proxied = config.proxied.unwrap_or(existing_record.proxied);
    let new_ttl = config.ttl.unwrap_or(existing_record.ttl);

    let ip_changed = existing_ip != public_ip;
    let settings_changed = new_proxied != existing_record.proxied || new_ttl != existing_record.ttl;

    if !ip_changed && !settings_changed {
        log::info!(
            "Cloudflare DNS record is already up to date with IP {}.",
            public_ip
        );
        if !args.dry_run {
            if let Err(e) = write_cached_ip(&cache_file_path, public_ip) {
                log::warn!("Failed to update local cache file: {}", e);
            } else {
                log::info!("Local IP cache file updated.");
            }
        }
        std::process::exit(0);
    }

    if ip_changed {
        log::info!(
            "IP change detected! Cloudflare: {} -> Current: {}",
            existing_record.content,
            public_ip
        );
    } else {
        log::info!(
            "IP unchanged ({}); syncing record settings (ttl: {} -> {}, proxied: {} -> {}).",
            public_ip,
            existing_record.ttl,
            new_ttl,
            existing_record.proxied,
            new_proxied
        );
    }

    if args.dry_run {
        log::info!(
            "[Dry Run] Would update record '{}' (id: {}) content '{}' -> '{}' (ttl: {} -> {}, proxied: {} -> {})",
            config.record_name,
            existing_record.id,
            existing_record.content,
            public_ip,
            existing_record.ttl,
            new_ttl,
            existing_record.proxied,
            new_proxied
        );
        std::process::exit(0);
    }

    log::info!("Updating Cloudflare DNS record...");
    match update_cloudflare_dns_record(
        &http_client,
        &config.zone_id,
        &existing_record.id,
        &config.record_name,
        &api_token,
        &public_ip.to_string(),
        new_ttl,
        new_proxied,
    ) {
        Ok(_) => {
            log::info!("Successfully updated Cloudflare DNS record.");
            if let Err(e) = write_cached_ip(&cache_file_path, public_ip) {
                log::warn!("Failed to update local cache file: {}", e);
            } else {
                log::info!("Local IP cache file updated.");
            }
        }
        Err(e) => {
            log::error!("Failed to update DNS record: {}", e);
            std::process::exit(1);
        }
    }
}
