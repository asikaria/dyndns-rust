# dyndns

A lightweight Cloudflare dynamic DNS client. It fetches the host's public IPv4
address and updates a single Cloudflare `A` record when it changes.

This README covers installing and running the `dyndns` binary. For configuration
reference, building, and cross-compiling, see the
[project repository](https://github.com/asikaria/dyndns-rust) and its `docs/`.

## Installation (Linux)

`scripts/build.sh` produces a `release/<target>/` directory containing the
binary, `config.toml`, `.env`, `README.md`, and `scripts/install.sh`. To deploy:

1. Copy the contents of the appropriate release subdirectory for the target's
   architecture (`release/arm64-linux/` or `release/amd64-linux/`) to a temp
   directory on the target host:

   ```bash
   scp release/arm64-linux/* user@host:/tmp/dyndns-install/
   ```

2. On the target, run the installer as root:

   ```bash
   sudo bash /tmp/dyndns-install/install.sh
   ```

The installer copies files into system locations, registers a systemd **system**
timer that runs the client **every 300 seconds**, and rewrites `cache_file` in
the installed config to the system cache path. Logs go to the journal.

### Installed file locations

| File | Location |
| --- | --- |
| Binary | `/usr/local/bin/dyndns` |
| Config | `/etc/dyndns/config.toml` |
| API token (`.env`) | `/etc/dyndns/.env` (mode `600`, root-only) |
| IP cache | `/var/cache/dyndns/dyndns.cache` (created/owned by systemd) |
| systemd units | `/etc/systemd/system/dyndns.service`, `dyndns.timer` |

### Managing the service

```bash
# View logs
journalctl -u dyndns.service

# Run an update immediately (outside the timer)
sudo systemctl start dyndns.service

# Stop and disable the scheduled updates
sudo systemctl disable --now dyndns.timer
```

## Usage

```
dyndns [OPTIONS]
```

Run with no options to fetch the current public IP and update Cloudflare if it
differs from the cached value:

```bash
dyndns
```

### Options

| Option | Description |
| --- | --- |
| `-c, --config <PATH>` | Path to the configuration TOML file. If omitted, searches `./config.toml`, then `/etc/dyndns/config.toml`. |
| `-f, --force` | Bypass the local IP cache and contact Cloudflare even if the IP is unchanged. Also syncs `proxied`/`ttl` changes from the config. |
| `-d, --dry-run` | Simulate the run: detect the IP and query Cloudflare, but make no changes to the cache or DNS record. |
| `-h, --help` | Print help. |
| `-V, --version` | Print version. |

### Examples

```bash
# Use an explicit config file
dyndns --config ~/.dyndns/config.toml

# Verify IP detection and Cloudflare connectivity without changing anything
dyndns --dry-run

# Force-push the current IP (and any proxied/ttl config changes) to Cloudflare
dyndns --force
```

The Cloudflare API token is read from a `.env` file (`CLOUDFLARE_API_TOKEN`)
located next to the config file or in the current directory — never from
`config.toml` or the process environment.

## Logging

Output goes to stdout/stderr via `env_logger`. The default level is `info`;
override it with `RUST_LOG`:

```bash
RUST_LOG=debug dyndns --dry-run
```

## More

See the [project repository](https://github.com/asikaria/dyndns-rust) for setup,
configuration reference, build/cross-compile instructions, and scheduling with
cron or systemd.
