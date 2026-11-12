# 5G MiFi Dashboard (UDX710-TOOLS)

[中文文档](README_CN.md) | [Changelog](CHANGELOG.md) | [Feature parity report](docs/FEATURE_PARITY.md)

A web-based management interface for 5G MiFi devices running on embedded Linux (aarch64).

**Current release: 3.1.0.** The `main` branch was fully rewritten following the *UDX710-TOOLS Rust / Native Frontend HTML5 Technical Report*: the backend moved from C + Mongoose + GLib/GIO to **Rust + Tokio + Axum**, and the frontend from Vue 3 + Vite + Tailwind to **plain HTML5 + CSS3 + JavaScript ES Modules**. No npm dependencies, no frontend build step, single binary with embedded static assets.

## Branches and releases

| Item | Detail |
| --- | --- |
| Maintained branch | `main` (the only remote branch today) |
| History | The `SZ50` branch mentioned in older docs no longer exists on the remote and is out of scope |
| CI | GitHub Actions cross-compiles `aarch64-unknown-linux-gnu`, artifact `udx710-tools-aarch64` |
| Default port | `6677` (CLI arg > `UDX_PORT` > `config.toml` > default) |

## What changed

| Version | Scope |
| --- | --- |
| **3.0.0** | Six-crate backend, native frontend, REST compatibility, security fixes, new plugin protocol, tests/CI/docs |
| **3.1.0** | White glassmorphism theme, animated SVG, three dialog variants, full-range responsive layout, frontend/backend watchdogs, feature parity audit, documentation sync |

Feature completeness was verified with repeatable scripts: **91 legacy C routes, 37 frontend endpoints, 27 component mappings, 16 menu items — zero uncovered**. See [docs/FEATURE_PARITY.md](docs/FEATURE_PARITY.md) and `tests/contract/parity.py`.

## Features

- **Modem & network**: IMEI/ICCID/operator, band & cell info, traffic stats and limits, interface monitoring, APN presets and custom profiles, airplane mode, charge control, scheduled reboot, system time.
- **Device**: USB mode switching (ECM/NCM/RNDIS) with an exclusive lock and rollback state machine, SMS with SSRF-protected webhooks, IPv6 port forwarding, Rathole tunneling, OTA update with TLS/digest/signature/atomic rollback, factory reset, reboot, device control, AT debug.
- **Extensions**: versioned JSON-UI plugin protocol with host rendering and permission checks, script management with non-executable storage, security questions, Token lifecycle and login rate limiting, audit log.
- **Web Shell**: preserved by explicit user request as a separate high-risk administrator diagnostic channel (`shell.execute` / `terminal.session` permissions, concurrency cap, timeouts, output cap, process-group cleanup, full audit). Disable with `[shell] enabled=false`.

## UI

- White glassmorphism theme with light / dark / system color schemes.
- Animated SVG aurora background, live brand mark and pulse wave (CSS + SMIL only, no JS animation loop).
- Three dialog variants: **Centered Modal Dialog** (desktop default), **Modal Bottom Sheet** (mobile default, draggable grabber), **Right Side Sheet** (logs, JSON details, long forms). Switchable and previewable in Settings.
- Responsive from 320px to 1920px, landscape phones, tablets and desktops; `100dvh`, `env(safe-area-inset-*)`, 44px touch targets, horizontally scrollable tables.
- Strict CSP retained: `script-src 'self'`, `style-src 'self'`, no `unsafe-eval` / `unsafe-inline`.

Full details in [docs/UI.md](docs/UI.md).

## Watchdog

| Layer | Detects | Recovery |
| --- | --- | --- |
| Backend OS thread | Tokio runtime freeze | Exit code 75, restarted by systemd / procd |
| Frontend heartbeat drift | Blocked UI main thread | Banner + auto reload (cancellable) |
| Frontend health probe | Backend unreachable / stalled | Retry loop + rebuild WS/SSE on recovery |

Config: `config.example.toml` `[watchdog]`. Status: `GET /api/health` → `watchdog`. Deployment: [docs/WATCHDOG.md](docs/WATCHDOG.md).

## Architecture

```
Cargo.toml                  Rust workspace (6 crates) + release profile
config.example.toml
crates/
  domain/                   Models, permissions, error semantics, state machines
  platform-linux/           Fixed-argv commands, Web Shell, sysfs/procfs, oFono D-Bus
  storage/                  SQLite (rusqlite bundled), DB worker, static parameterized SQL
  services/                 Business services, AT queue, USB state machine, OTA, plugins, watchdog
  api/                      Axum routes, middleware, handlers, WS/SSE, embedded static assets
  app/                      Entry point, Tokio runtime, graceful shutdown
web/                        Static frontend embedded by rust-embed
  index.html                SPA shell + strict CSP
  css/                      tokens / layout / components / glass / sheets / responsive
  js/                       core / components / pages / utils / i18n (ES Modules)
  icons/                    sprite.svg + animated SVGs
migrations/001_init.sql     Idempotent schema migration
tests/contract/             95-endpoint API contract + verify.py + parity.py
crates/api/tests/           Security integration tests
docs/                       Architecture, contract, migration, plugin protocol, security, UI, watchdog
examples/plugins/           Sample plugin using the new protocol
packaging/                  start.sh, systemd unit, procd init, deployment notes
```

Legacy `src/` (C) and `web/src/` (Vue), `web/package.json`, `vite.config.js` are historical and unused by 3.x builds.

## Deploy

```sh
cp config.example.toml config.toml
./start.sh            # default port 6677
./start.sh 8080       # custom port
# open http://<device-ip>:6677
```

```sh
# systemd
cp packaging/udx710-tools.service /etc/systemd/system/
systemctl daemon-reload && systemctl enable --now udx710-tools

# OpenWrt procd
cp packaging/udx710-tools.init /etc/init.d/udx710-tools
chmod 755 /etc/init.d/udx710-tools
/etc/init.d/udx710-tools enable && /etc/init.d/udx710-tools start
```

Default password is `admin` — **change it immediately** via `/api/auth/password`. See [packaging/README.md](packaging/README.md) for rollback instructions.

## Build from source

```sh
cargo test --workspace
python3 tests/contract/verify.py
python3 tests/contract/parity.py
cargo run -p udx-app -- 6677

rustup target add aarch64-unknown-linux-gnu
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc \
cargo build --release --target aarch64-unknown-linux-gnu -p udx-app
```

Requires Rust stable and `gcc-aarch64-linux-gnu` for cross builds. Node.js is not required. Release profile: `opt-level="s"`, LTO, `codegen-units=1`, `panic="abort"`, `strip=true`. **No fixed memory-saving ratio is promised** — performance must be measured on real hardware.

## Verification

| Check | Result |
| --- | --- |
| `cargo test --workspace` | 53 passed / 0 failed (5 watchdog unit tests, 11 API integration tests) |
| `tests/contract/verify.py` | PASS: 95 endpoints, all 90 legacy C routes matched, 0 unmatched, 69 write endpoints require auth |
| `tests/contract/parity.py` | PASS: 91 routes / 37 endpoints / 27 components / 16 menus, 0 uncovered |
| Frontend JS | all files pass `node --check` |
| Live process smoke test | health, login, unauthorized blocking, Web Shell, WS handshake + Token revocation, audit, CSP |

**Needs real hardware** (no oFono D-Bus in the sandbox; device actions return `device_verified: false` rather than faking success): Modem/SMS/USB/Rathole/IPv6 behavior, D-Bus reconnect, OTA power-loss rollback, watchdog respawn latency, RSS/PSS, 72-hour stability, memory pressure.

## API and security

- Legacy path spellings retained (`/api/claen/cron`, `/api/get/Total`); new fields do not change old semantics.
- All write endpoints require authentication; the old `/api/device_control` POST/OPTIONS auth bypass is fixed and CORS preflight returns 204 without executing any action.
- Error body stays compatible: `{ "status": "error", "message": "..." }`; internal details are never echoed.
- Static parameterized SQL, fixed-argv internal commands, OTA TLS/digest/signature, webhook SSRF protection, no arbitrary plugin JS/HTML, access logs contain only `method/path/status/latency_ms` (no query tokens, headers or bodies).

See [docs/API_CONTRACT.md](docs/API_CONTRACT.md) and [docs/SECURITY.md](docs/SECURITY.md).

## Documentation

| Doc | Content |
| --- | --- |
| [CHANGELOG.md](CHANGELOG.md) | Release notes |
| [docs/UI.md](docs/UI.md) | Glass theme, animated SVG, dialog variants, responsive breakpoints |
| [docs/WATCHDOG.md](docs/WATCHDOG.md) | Watchdogs and supervisor self-healing |
| [docs/FEATURE_PARITY.md](docs/FEATURE_PARITY.md) | Legacy feature parity evidence |
| [docs/API_CONTRACT.md](docs/API_CONTRACT.md) | Endpoint table and compatibility policy |
| [docs/SECURITY.md](docs/SECURITY.md) | Security audit mapping and threat model |
| [docs/PLUGIN_PROTOCOL.md](docs/PLUGIN_PROTOCOL.md) | JSON UI plugin protocol and migration |
| [docs/MIGRATION.md](docs/MIGRATION.md) | Migration phases and hardware acceptance |
| [packaging/README.md](packaging/README.md) | Deployment and rollback |
| [tests/README.md](tests/README.md) | Tests and contract scripts |

`docs/screenshot*.png` are 2.x (Vue 3) archives; 3.x uses the white glass theme.

## License

**GPLv3** — see [LICENSE](LICENSE). You may use, modify and distribute; you must keep copyright notices, publish source when distributing, and use the same license. Closed-source commercial use and relicensing are not permitted.

## Credits

Thanks to 等不住 (AT commands), 黑衣剑士 (USB mode switching), Voodoo (GLib build environment), 1orz ([project-cpe](https://github.com/1orz/project-cpe)), and LeoChen (original author).

QQ group: 1029148488. Issues and pull requests are welcome.
