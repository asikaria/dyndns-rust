# Setup & Usage Guide: Rust DDNS Client

This guide explains how to compile, configure, run, and schedule the custom dynamic DNS client.

---

## 1. Compilation & Installation

### A. Local Compilation
If you are compiling directly on your target machine (e.g. compiling on the Raspberry Pi 5 or on the EC2 instance):

Ensure you have Rust and Cargo installed. Then, compile the release binary:

```bash
cargo build --release
```

The compiled binary will be located at `target/release/dyndns`. You can copy it to your system path:

```bash
sudo cp target/release/dyndns /usr/local/bin/
```

### B. Cross-Compilation for ARM64 / AArch64 (Raspberry Pi 5 & AWS t4g.small)
Both the **Raspberry Pi 5** (running 64-bit OS) and the **AWS t4g.small EC2** instance (powered by AWS Graviton processor) run 64-bit ARM Linux. The corresponding target architecture is:
`aarch64-unknown-linux-gnu`

Because our crate is built using the pure-Rust TLS stack (`rustls-tls` instead of standard OpenSSL), cross-compiling from macOS is extremely easy as there is no need to compile or link C-based OpenSSL libraries.

Choose one of the following two standard ways to cross-compile from macOS:

#### Method 1: Using `cross` (Recommended - Requires Docker)
`cross` uses Docker container runtimes containing the target C compiler and toolchain, eliminating the need to set up local compilers.

1.  Install `cross`:
    ```bash
    cargo install cross --git https://github.com/cross-rs/cross
    ```
2.  Compile target:
    ```bash
    cross build --target aarch64-unknown-linux-gnu --release
    ```
3.  The binary will be located at:
    `target/aarch64-unknown-linux-gnu/release/dyndns`

#### Method 2: Using `cargo-zigbuild` (Fastest - Requires Zig)
`cargo-zigbuild` uses the Zig compiler as a linker, which is highly portable and doesn't require Docker.

1.  Install Zig and `cargo-zigbuild` via Homebrew:
    ```bash
    brew install zig
    cargo install cargo-zigbuild
    ```
2.  Add the Rust compile target:
    ```bash
    rustup target add aarch64-unknown-linux-gnu
    ```
3.  Compile target:
    ```bash
    cargo zigbuild --target aarch64-unknown-linux-gnu --release
    ```
4.  The binary will be located at:
    `target/aarch64-unknown-linux-gnu/release/dyndns`

### C. Deploying to Targets
Copy the cross-compiled binary to your target hosts using `scp`:

```bash
# Deploy to Raspberry Pi 5
scp target/aarch64-unknown-linux-gnu/release/dyndns user@raspberrypi.local:/tmp/
ssh user@raspberrypi.local "sudo mv /tmp/dyndns /usr/local/bin/"

# Deploy to AWS t4g.small EC2
scp target/aarch64-unknown-linux-gnu/release/dyndns ec2-user@your-ec2-ip:/tmp/
ssh ec2-user@your-ec2-ip "sudo mv /tmp/dyndns /usr/local/bin/"
```

### D. Cross-Compilation from Windows 11
If your development machine runs Windows 11, Zig and `cargo-zigbuild` work natively. You also have the option of compiling via WSL 2 (Windows Subsystem for Linux) or Docker.

#### Option 1: Using `cargo-zigbuild` (Native Windows - No Docker)
1.  Install Zig via a Windows package manager:
    *   **winget**: `winget install zig.zig`
    *   **scoop**: `scoop install zig`
    *   **chocolatey**: `choco install zig`
2.  Install `cargo-zigbuild`:
    ```cmd
    cargo install cargo-zigbuild
    ```
3.  Add the target:
    ```cmd
    rustup target add aarch64-unknown-linux-gnu
    ```
4.  Build:
    ```cmd
    cargo zigbuild --target aarch64-unknown-linux-gnu --release
    ```
5.  The binary will be located at `target/aarch64-unknown-linux-gnu/release/dyndns`.

#### Option 2: Using WSL 2 (Windows Subsystem for Linux)
Since Windows 11 has WSL 2 built-in, you can compile inside a native Linux shell (like Ubuntu):
1.  Open your WSL terminal.
2.  Install Rust inside WSL:
    ```bash
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    ```
3.  Install the cross-compiler toolchain:
    ```bash
    sudo apt update && sudo apt install -y gcc-aarch64-linux-gnu
    ```
4.  Add the Rust compile target:
    ```bash
    rustup target add aarch64-unknown-linux-gnu
    ```
5.  Configure Cargo to use the ARM64 linker by creating a `.cargo/config.toml` in your project folder (or system-wide in `~/.cargo/config.toml`):
    ```toml
    [target.aarch64-unknown-linux-gnu]
    linker = "aarch64-linux-gnu-gcc"
    ```
6.  Build:
    ```bash
    cargo build --target aarch64-unknown-linux-gnu --release
    ```

#### Option 3: Using `cross` (Requires Docker Desktop for Windows)
If you have Docker Desktop installed on Windows 11:
1.  Install `cross`:
    ```cmd
    cargo install cross --git https://github.com/cross-rs/cross
    ```
2.  Build:
    ```cmd
    cross build --target aarch64-unknown-linux-gnu --release
    ```

---

## 2. Configuration Setup

The program requires a TOML configuration file. By default, it searches the following paths in order:
1.  `./config.toml` (Current working directory)
2.  `/etc/dyndns/config.toml` (System-wide path used by the systemd install)

You can also point at any path explicitly with `--config <PATH>`.

### A. config.toml
Create `config.toml` based on the `config.toml.example` template. The packaging
step in `scripts/build.sh` picks it up from the project root and ships it; the
systemd installer places it at `/etc/dyndns/config.toml`.

```toml
# target zone and record
zone_id = "your_cloudflare_zone_id"
record_name = "home.example.com"

# Optional overrides (preserves current Cloudflare dashboard settings if omitted)
# proxied = false
# ttl = 120

# local cache file (relative to the cwd; the installer overrides this)
cache_file = "./dyndns.cache"

# fallback plain-text IPv4 fetchers
endpoints = [
    "https://checkip.amazonaws.com",
    "https://icanhazip.com",
    "https://api.ipify.org",
    "https://ifconfig.me"
]
```

> [!NOTE]
> The systemd installer (`scripts/install.sh`) rewrites `cache_file` to
> `/var/cache/dyndns/dyndns.cache` and lets systemd manage that directory, so the
> value above only matters for manual runs.

### B. Configure Cloudflare API Credentials
The client requires a `.env` file containing your Cloudflare API token. Credentials are not read from `config.toml` or directly from the process environment. The token is loaded from a `.env` next to the config file or in the current directory.

1.  Go to the Cloudflare Dashboard -> **My Profile** -> **API Tokens**.
2.  Create a Token using the **Edit zone DNS** template.
3.  Set the permission: **Zone - DNS - Edit** for your target domain.
4.  Create a `.env` file (in the project root for packaging, or directly at the target) containing:
    ```text
    CLOUDFLARE_API_TOKEN="your_actual_api_token"
    ```

The installer writes this to `/etc/dyndns/.env` with mode `600` (root-only).

---

## 3. Command Line Usage

Run the program manually to verify your setup:

```bash
# General help
dyndns --help

# Run with defaults (searches ./config.toml then /etc/dyndns/config.toml)
dyndns

# Specify a custom config file path
dyndns --config /path/to/custom/config.toml

# Dry-run mode: verify IP detection and Cloudflare connection without making changes
dyndns --dry-run

# Force update: bypass the cache check and force-update Cloudflare
dyndns --force
```

> [!TIP]
> After modifying configuration parameters in `config.toml` (such as `proxied` or `ttl`), run `dyndns --force` once to synchronize the settings immediately, even if your public IP address has not changed.

---

## 4. Installation & Scheduling

### Option A: Automated system install (Recommended)
`scripts/build.sh` stages a `release/<target>/` directory (`arm64-linux` or `amd64-linux`) containing the binary, `config.toml`, `.env`, `README.md`, and `scripts/install.sh`. Copy the matching directory's contents to a temp dir on the target and run the installer as root:

```bash
scp release/arm64-linux/* user@host:/tmp/dyndns-install/
ssh user@host
sudo bash /tmp/dyndns-install/install.sh
```

The installer:
*   installs the binary to `/usr/local/bin/dyndns`,
*   writes `config.toml` and `.env` (mode `600`) to `/etc/dyndns/`,
*   pins `cache_file` to `/var/cache/dyndns/dyndns.cache`, managed by the unit's `CacheDirectory`,
*   installs a systemd **system** service and timer to `/etc/systemd/system/` that run dyndns every **300 seconds**.

Manage the service:
```bash
journalctl -u dyndns.service               # view logs
sudo systemctl start dyndns.service        # run an update now
sudo systemctl disable --now dyndns.timer  # stop scheduled updates
```

---

### Option B: Cron Job
If you prefer cron over systemd, install the binary and config manually (`/usr/local/bin/dyndns`, `/etc/dyndns/config.toml`, `/etc/dyndns/.env`) and add a root crontab entry (`sudo crontab -e`) to run every 5 minutes:

```text
*/5 * * * * /usr/local/bin/dyndns --config /etc/dyndns/config.toml > /dev/null
```
