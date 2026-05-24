# NeuroBoot

**Language:** **English** · [简体中文](README.zh-CN.md)

> **Neuro = AI · Boot = bootable USB.** A Windows PE rescue USB with an embedded native AI assistant that understands Chinese, runs offline on CPU, calls diagnostic tools, and asks for confirmation before destructive operations.

[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-Windows%2010%2F11-0078D6)](docs/BUILD.md)
[![Build](https://img.shields.io/badge/PE%20Build-WinPE%20amd64-success)](docs/BUILD.md)

Boot a stick → land in WinPE → the desktop **is** a thinking AI assistant (`NeuroBoot.exe`).
Tell it the symptom ("blue screen since yesterday", "boot loop after a driver update"); it picks
the right diagnostic tools, runs them, and walks you through the fix — pausing for a confirmation
popup before anything that can lose data.

## Why this exists

Traditional PE rescue USBs (微PE / Sergei Strelec / Hiren's BootCD PE / 老毛桃) hand the user a
desktop full of icons (DiskGenius, Ghost, regedit, …) and assume they know which tool to launch
for what symptom. NeuroBoot inverts that: the user describes the symptom in natural Chinese, the
AI agent picks tools via OpenAI-compatible function calling, and a hard confirmation gate makes
destructive operations explicit and reviewable.

## Features

- **GUI** — Rust + `egui`/`eframe`, glow/OpenGL backend with embedded Noto Sans SC font (2.4 MB
  baked into the exe via `include_bytes!`)
- **Software rendering fallback** — Mesa3D llvmpipe ships next to the exe so the GUI works on PE
  even without GPU drivers
- **Local model** — Qwen3-4B-Instruct-2507 Q4_K_M (2.33 GB) via llama.cpp `llama-server`, CPU
  inference, no GPU required
- **Agent** — hand-rolled tool-use loop (no third-party SDK), OpenAI function-calling compatible,
  5-round limit, automatic context truncation to fit server ctx
- **Tools (v1.0)** — 4 registered: 3 safe (`list_disks` / `read_system_info` /
  `read_event_log_errors`) + 1 dangerous (`delete_path`, with confirmation popup and a hard-coded
  blocklist that refuses whole-drive deletes)
- **A+C dual-endpoint** (v1.0.1+) — three-tier config (env var > `config.json` on the USB > built-in
  defaults). On startup, probes the remote OpenAI-compatible endpoint via HEAD `/v1/models`; if
  reachable use it, otherwise fall back to local llama-server. One-click switch in the top bar.
- **Vision multimodal** (v1.0.1+) — `+ Image` button (rfd Win32 file dialog → base64 data URL →
  OpenAI vision schema). Heuristic VL model detection (gpt-4o / claude-3 / qwen-vl /
  deepseek-vl / glm-4v / gemini / pixtral / …) auto-disables the button for non-VL endpoints.
- **Status bar** (v1.0.1+) — local clock + RAM usage + local IP, 5 s cached refresh via direct
  Win32 FFI (`GetLocalTime` / `GlobalMemoryStatusEx`) — no extra crates.
- **System launchers** (v1.0.1+) — top-bar buttons for `cmd` (opens new console in
  `X:\NeuroBoot`) and `File Manager` (tries `explorer.exe`, falls back to `cmd dir` in PE which
  doesn't ship explorer)
- **Power controls** (v1.0.1+) — Reboot / Shutdown / Exit-to-cmd buttons (`wpeutil reboot|shutdown`
  + `std::process::exit(0)`), each guarded by a confirmation popup. PE users would otherwise have
  to hold the power button.
- **PE compatibility** — `+crt-static` linking removes the VCRUNTIME140 dependency; UCRT
  redistributable DLLs (17 files, 1.9 MB) are bundled next to llama-server.exe to survive PE's
  missing UCRT runtime; UTF-8 throughout for Chinese text.

## Status

| Version | ISO size | Date | Notes |
|---|---|---|---|
| v1.0    | 2.89 GB | 2026-05-23 | First bootable build, USB real-machine tested |
| v1.0.1+ | 2.93 GB | 2026-05-24 | 4 P0 fixes from USB real test + 5 user-feedback additions (status bar, system launchers, power controls, image upload, healthcheck startup) |
| v2 Stage A | – (no rebuild) | 2026-05-24 | Markdown rendering (egui_commonmark) + 8 new safe diagnostic tools (12 total) |
| **v2 Stage 1** | done (code) | 2026-05-24 | System prompt hardening + tool description spec rewrite + Q5_K_M (2.69 GB) quant upgrade |
| **v2 Stage 2** | done (code) | 2026-05-24 | Streaming SSE + tool_calls accumulation + llama.cpp #20198 compat + cancel button |
| **v2 Stage 3** | done (code) | 2026-05-24 | tool_result clearing + audit log JSONL + ToolError kind enum |
| **v2 Stage 4** | done (code) | 2026-05-24 | 5 new dangerous tools (chkdsk/sfc/dism/Defender/bootrec) + delete_path → move-to-trash + --readonly + preflight |
| **v2 Stage 5** | MVP done | 2026-05-24 | Local vision model integration via existing ⚙ + helper script + docs (full lazy-spawn = v2.x) |
| **v2 Stage 6** | code done | 2026-05-24 | NTPWEdit/TestDisk/smartctl wrappers (binaries downloaded per docs/BUILD.md) |
| **v2 Stage 7** | done (code) | 2026-05-24 | One-click full check + skill system + --forensic + dangerous dialog red frame |
| **v2 Stage 8** | done (code) | 2026-05-24 | Hand-rolled MCP server over stdio (--mcp-server), 12 safe tools exposed, no tokio dep |
| **v2 ISO rebuild** | pending | — | All v2 changes ready; one admin PowerShell build pass produces NeuroBoot.iso ~3.3 GB |

See **[docs/TODO-v2.md](docs/TODO-v2.md)** for the **8-stage v2 roadmap** (~5.5-9 days of work,
explicitly excluding Microsoft Phi/QMR/Foundry Local per 2026-05 research), and
**[docs/RESEARCH-2026-05.md](docs/RESEARCH-2026-05.md)** for the consolidated research findings
(VL model selection, agent architecture, Microsoft ecosystem analysis) from ~50 web searches.

## Hardware requirements (target machine running the USB)

| Item | Required | Recommended |
|---|---|---|
| RAM | ≥ 4 GB | ≥ 8 GB |
| Secure Boot | **Disabled** when booting PE | — |
| **Mouse / Keyboard** | **Wired USB or 2.4 GHz USB-receiver wireless** | — |

**⚠ Bluetooth mice / keyboards are not supported** — Windows PE does not include the Bluetooth
stack (Microsoft ADK design limit, no software workaround). Grab a wired USB mouse or a
2.4 GHz dongle-receiver wireless mouse before booting the USB.

**⚠ Chinese IME input** — PE ships without IME framework. NeuroBoot ships 6 built-in
quick-question buttons (blue screen / disk issues / network / slow boot / file recovery / system
repair) plus reads `NeuroBoot.prompts.txt` from any non-X: USB partition (one candidate question
per line). Full pinyin IME is on the v1.1 roadmap.

**⚠ Online AI endpoint config** — click the ⚙ gear button in the top bar. Fill endpoint URL,
model name, API key. Save to USB → written as `NeuroBoot.config.json` at the first writable
non-X: drive root. Auto-loaded on next boot.

## Quick start (build the ISO yourself)

Detailed setup in **[docs/BUILD.md](docs/BUILD.md)**. TL;DR if you already have ADK + Rust +
Visual Studio 2026 + Mesa-dist-win extracted + Qwen GGUF downloaded:

```powershell
# In an Administrator PowerShell:
PowerShell -NoProfile -ExecutionPolicy Bypass `
  -File C:\NeuroBoot\pe-build\build-scripts\99-build-all.ps1
```

This one-shot pipeline runs `cargo build --release` (with `RUSTFLAGS=-C target-feature=+crt-static`)
→ `copype amd64` → DISM mount → adds 6 WinPE OCs (WMI / NetFx / Scripting / PowerShell /
StorageWMI / FontSupport-ZH-CN) → copies payload (NeuroBoot + Mesa + llama.cpp + Qwen GGUF +
17 CRT redist DLLs) → unmount/commit → MakeWinPEMedia /ISO. Total time 3–20 minutes depending
on cargo cache state.

Output: `pe-build/output/NeuroBoot.iso` (~2.93 GB).

## Key artifacts

| Artifact | Path | Size |
|---|---|---|
| Final ISO (v1.0.1+) | `pe-build/output/NeuroBoot.iso` | ~2.93 GB |
| Rust release exe | `app/target/release/neuroboot.exe` | ~11.71 MB (crt-static, with rfd) |
| PE payload | `pe-build/payload/neuroboot/` | ~70 MB |
| CRT redist (v1.0.1+) | `pe-build/payload/crt-redist/` | 1.9 MB (17 DLLs) |
| Qwen GGUF | `models/Qwen3-4B-Instruct-2507-Q4_K_M.gguf` | 2.33 GB |
| llama.cpp b9294 (CPU) | `tools-dev/llama-cpp/b9294/` | ~50 MB |
| Mesa-dist-win 26.1.1 | `tools-dev/mesa-extract/x64/` | opengl32.dll + libgallium_wgl.dll |
| Ventoy 1.1.12 | `tools-dev/ventoy/ventoy-1.1.12/` | 15.94 MB |
| USB config templates | `docs/usb-templates/` | NeuroBoot.config.json + prompts.txt |

## Documentation map

- **[docs/BUILD.md](docs/BUILD.md)** — full zero-to-ISO build guide
- **[docs/KNOWN-ISSUES.md](docs/KNOWN-ISSUES.md)** — every trap we hit + the workaround (19 numbered entries + summary)
- **[docs/TODO-v1.0.1-fixes.md](docs/TODO-v1.0.1-fixes.md)** — v1.0.1 P0 fix checklist (all checked)
- **[docs/TODO-v2.md](docs/TODO-v2.md)** — v2 roadmap (P0/P1/P2 prioritized)
- **[docs/usb-templates/](docs/usb-templates/)** — sample `NeuroBoot.config.json` + `NeuroBoot.prompts.txt` to drop on the USB

## License

This project is licensed under the **[Apache License 2.0](LICENSE)**.

- ✅ Commercial use, modification, distribution, private use — allowed
- ✅ Explicit patent grant (with retaliation clause — suing a contributor for patent infringement
  voids your license)
- ⚠ Must preserve copyright notice + mark modified files + **must not use the "NeuroBoot" or
  "神启" trade names** to promote forks
- Third-party component attributions (Noto Sans SC, llama.cpp, Mesa, Qwen model weights, Ventoy,
  Microsoft CRT redistributables, …) are in **[NOTICE](NOTICE)**.

## Contributing

Before opening an issue or PR, please scan:

- [docs/KNOWN-ISSUES.md](docs/KNOWN-ISSUES.md) — is it a known PE / build trap?
- [docs/TODO-v2.md](docs/TODO-v2.md) — already on the roadmap?

Code conventions:
- Comments are written in Chinese to match the existing codebase (this is a project mainly
  serving Chinese-speaking PE users)
- Each new module should ship with unit tests (`cargo test`, currently 31 tests)
- After changes touching native FFI / linking, verify the release exe stays PE-compatible:
  `dumpbin /DEPENDENTS app\target\release\neuroboot.exe` must not list `VCRUNTIME140.dll`
  nor `api-ms-win-crt-*.dll`. Use `tools-dev/build-release.ps1` (sets `RUSTFLAGS` correctly).
- For PowerShell scripts, keep them pure ASCII OR save as UTF-8 with BOM — PS 5.1 on
  zh-CN Windows decodes BOM-less Chinese as GBK and breaks. (See
  [KNOWN-ISSUES.md #19](docs/KNOWN-ISSUES.md).)
