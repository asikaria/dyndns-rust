# Implementation Plan - Custom DDNS Client (Rust)

Establish a lightweight, robust, and fast command-line Rust tool (`dyndns`) that updates a Cloudflare A record when the host's public IPv4 changes, using plain-text fallback IP fetchers, local caching, and custom TOML settings.

## Proposed Changes

### Repository Initialization

We will initialize a new binary-only Rust application in the current directory.

#### [NEW] [Cargo.toml](file:///Users/atul/dev/dyndns/Cargo.toml)
Specify dependencies and features for compilation.

*   `clap` (with `derive` feature): CLI parser.
*   `reqwest` (with `blocking`, `rustls-tls`, disabling `default-features`): Secure HTTP client.
*   `serde` & `serde-json`: Deserialization of config and Cloudflare API payloads.
*   `toml`: TOML config parser.
*   `log` & `env_logger`: Logging framework.

#### [NEW] [config.toml.example](file:///Users/atul/dev/dyndns/config.toml.example)
A reference/template configuration file.

```toml
# Cloudflare Credentials & Targets
zone_id = "your-zone-id"
record_name = "home.example.com"

# Optional overrides (preserves Cloudflare settings if omitted)
# proxied = false
# ttl = 300 # minimum is 60s, or 1 for automatic

# Client Caching & Fetchers
cache_file = "/var/tmp/dyndns.cache"
endpoints = [
    "https://icanhazip.com",
    "https://api.ipify.org",
    "https://checkip.amazonaws.com",
    "https://ifconfig.me"
]
```

#### [NEW] [src/main.rs](file:///Users/atul/dev/dyndns/src/main.rs)
Contains the core code of the binary:
1.  **CLI Arg Definition**: Struct `Args` representing CLI options (`--config`, `--force`, `--dry-run`).
2.  **Config Definitions**: Struct `Config` representing the TOML file structure.
3.  **Config Loader**: A helper function to parse TOML, load `CLOUDFLARE_API_TOKEN` from a `.env` file, validate inputs, and merge values.
4.  **IP Fetcher Loop**: Iterates over configured IP endpoints, performs GET, trims whitespace, parses with `std::net::Ipv4Addr`. Returns first valid IP.
5.  **State Manager**: Reads/writes IP cache file.
6.  **Cloudflare Client API**:
    *   Query: `GET /client/v4/zones/{zone_id}/dns_records?name={record_name}`. Parses response to retrieve target `record_id`, current `content` (IP), and existing `proxied`/`ttl` settings.
    *   Mutation: `PATCH /client/v4/zones/{zone_id}/dns_records/{record_id}`. Sends updated IP and preserves (or overrides) `proxied` / `ttl` values.
7.  **Main Execution Loop**: Coordinates configuration loading, IP fetching, cache comparison, Cloudflare API interaction, cache updating, and logging.

---

## Verification Plan

### Automated Checks
*   Verify code format: `cargo fmt --check`
*   Lint code: `cargo clippy -- -D warnings`
*   Verify compilation: `cargo build --release`

### Manual Verification
1.  **Configuration Verification**: Test with a malformed `config.toml` to ensure the program exits with clear errors.
2.  **Dry Run Test**: Run `cargo run -- --config config.toml --dry-run` to verify it fetches the public IP and queries Cloudflare without writing to the cache or updating the DNS record.
3.  **Cache Hit Test**: Run `cargo run -- --config config.toml` twice. The second run should log "IP unchanged (cache hit)" and exit without querying Cloudflare.
4.  **Force Cache Bypass Test**: Run `cargo run -- --config config.toml --force` to ensure it bypasses the cache file check and reaches out to Cloudflare.
