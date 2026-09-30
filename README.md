```text
                        █████  █████████                             
                       ░░███  ███░░░░░███                            
  ██████   ██████    ███████ ░███    ░░░  ████████   ██████   ██████ 
 ███░░███ ░░░░░███  ███░░███ ░░█████████ ░░███░░███ ███░░███ ███░░███
░███ ░░░   ███████ ░███ ░███  ░░░░░░░░███ ░███ ░███░███████ ░███ ░░░ 
░███  ███ ███░░███ ░███ ░███  ███    ░███ ░███ ░███░███░░░  ░███  ███
░░██████ ░░████████░░████████░░█████████  ░███████ ░░██████ ░░██████ 
 ░░░░░░   ░░░░░░░░  ░░░░░░░░  ░░░░░░░░░   ░███░░░   ░░░░░░   ░░░░░░  
                                          ░███                       
                                          █████                      
                                         ░░░░░                       
```

<p align="center">
  <a href="https://github.com/UniverLab/cadspec/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/UniverLab/cadspec/ci.yml?branch=main&style=for-the-badge&label=CI" alt="CI"/></a>
  <img src="https://img.shields.io/badge/Status-Active-27AE60?style=for-the-badge" alt="Status"/>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-2E8B57?style=for-the-badge" alt="License"/></a>
</p>

<p align="center"><a href="https://univerlab.org/cadspec/">Project home — the UniverLab experiment page</a></p>

cadspec is a **CAD as code** CLI tool and Rust library for declarative CAD modeling. Write geometry as code in `.cf` TOML format, watch it live in the browser, and compile to DXF — built for humans and AI agents working together.

---

## Features

- **📐 Declarative Geometry** — Define architectural elements (lines, rects, circles, arcs, polylines, text, dimensions) in TOML `.cf` files. Deterministic, reproducible, version-controlled.
- **🛠️ Construction Tools** — `[[array]]` (linear and polar) and `[[mirror]]` expand into concrete primitives at build time; copies get derived ids.
- **📏 Styled Dimensions** — Auto-measured labels with configurable `text_size`, `precision`, `show_units`, and `offset`.
- **🔴 Live Preview** — `cadspec serve` runs a local server with pan/zoom, auto-reload on save (SSE), click-to-inspect, per-layer ghost/hide, 3D view, and build-error overlay.
- **🔗 Layer System** — Organize geometry by layer with custom names, colors, and line weights.
- **📄 DXF Export** — Compile `.cf` → DXF (AutoCAD-compatible). Full layer support, LWPOLYLINE polygons, LINE/CIRCLE/ARC geometry, TEXT annotations, DIMENSION entities, and SOLID-filled regions.
- **🖼️ Previews for Agents** — Raster PNG + metadata JSON (entity bounding boxes) and full-fidelity SVG with real text, dimensions, line styles, and highlights.
- **✅ Validation Engine** — `cadspec check` validates geometry and constraints; `--json` for tooling.
- **🔄 Formatting** — `cadspec fmt` normalizes `.cf` files. `--check` mode for CI.
- **🔍 DXF Import** — `cadspec import drawing.dxf` migrates existing drawings into `.cf` layers.

---

## Installation

### Quick install (recommended)

**Linux / macOS:**

```bash
curl -fsSL https://install.univerlab.org/cadspec | sh
```

**Windows (PowerShell):**

```powershell
irm https://raw.githubusercontent.com/UniverLab/cadspec/main/scripts/install.ps1 | iex
```

### From source

```bash
git clone https://github.com/UniverLab/cadspec.git
cd cadspec
cargo build --release
# Binary at target/release/cadspec
```

### GitHub Releases

Check the [Releases](https://github.com/UniverLab/cadspec/releases) page for precompiled binaries (Linux x86_64, macOS x86_64/ARM64, Windows x86_64).

### Uninstall

```bash
rm -f ~/.local/bin/cadspec
```

---

## Quick Start

```bash
cadspec new casa && cd casa
cadspec serve --open        # live preview in the browser
```

Edit any `.cf` file — the browser updates on every save. When the design is right:

```bash
cadspec build                        # default output.dxf
cadspec build --output plano.dxf     # custom output path
cadspec build --layer muros          # compile single layer
```

## Documentation

Full documentation lives in [`docs/`](docs/): installation, quick start, the
`.cf` format, live preview, building & export, working with agents, and the
complete CLI reference.

---

## Commands

| Command | Description |
|---------|-------------|
| `cadspec new <name>` | Create a new project with multi-layer scaffold |
| `cadspec init` | Initialize cadspec in current directory |
| `cadspec build` | Compile project to DXF |
| `cadspec build --check` | Validate project and constraints without generating DXF |
| `cadspec build --output <path>` | Compile to custom output path |
| `cadspec build --layer <name>` | Compile specific layer only |
| `cadspec serve` | Live preview server — browser auto-reloads on save |
| `cadspec serve --open --port 4377` | Open browser automatically on a custom port |
| `cadspec check` | Validate with project metadata and layer colors |
| `cadspec check --json` | Machine-readable validation report |
| `cadspec layers` | List layers with entity counts and colors |
| `cadspec layers --json` | Machine-readable layer listing |
| `cadspec schema` | Print the full `.cf` language reference (markdown) |
| `cadspec preview` | Faithful PNG render + metadata JSON |
| `cadspec preview --format svg` | Vector SVG preview (same renderer) |
| `cadspec preview --highlight <id1,id2>` | Amber markers around specific entities |
| `cadspec preview --width 1024 -H 768` | Custom resolution preview |
| `cadspec preview --layer <name>` | Preview specific layer only |
| `cadspec fmt` | Format .cf files (normalize whitespace) |
| `cadspec fmt --check` | Check formatting without modifying (CI) |
| `cadspec watch` | Auto-rebuild on file changes |
| `cadspec import <file.dxf>` | Import DXF into `.cf` layers + `project.toml` |
| `cadspec import <file.dxf> --layer <name>` | Import only one DXF layer |
| `cadspec view` | Open the project in the configured viewer |
| `cadspec view --layer <name>` | Open only one layer in the viewer |
| `cadspec config set <key> <value>` | Set global defaults (`author`, `units`) |
| `cadspec config show` | Show global defaults |

### Live preview controls (`cadspec serve`)

- Scroll → zoom (centered on cursor) · drag → pan · double-click / `F` → fit
- **Click an entity** → inspector with its source TOML block; `copy for agent` produces a ready-made targeted-edit prompt
- Layer panel (or keys `1`-`9`) → cycle each layer **on → ghost → off**; ghost mode traces one floor plan over another
- `3D` button (or key `3`) → stacked exploded view of the layers
- Browser auto-reloads on every `.cf` / `project.toml` save (SSE)
- Build errors render as an overlay with file/line detail — the loop never breaks

---

## .cf Format

```toml
[layer]
name = "muros"
color = "#FFFFFF"

[[line]]
id = "ln-001"
from = [0.0, 0.0]
to = [8.5, 0.0]
weight = 0.50

[[rect]]
id = "rc-001"
origin = [1.0, 1.0]
width = 3.5
height = 4.0

[[circle]]
id = "ci-001"
center = [4.0, 3.0]
radius = 0.5

[[arc]]
id = "ac-001"
center = [2.0, 2.0]
radius = 0.9
from_angle = 0.0
to_angle = 90.0

[[polyline]]
id = "pl-001"
points = [[0.0, 0.0], [5.0, 0.0], [5.0, 3.0], [0.0, 3.0]]
closed = true

[[text]]
id = "tx-001"
position = [4.0, 3.0]
content = "SALA"
size = 0.2
font = "serif"
rotation = 15.0

[[dim]]
id = "dm-001"
from = [0, 0]
to = [5, 0]
offset = 0.5
```

### Supported Primitives

`line`, `polyline`, `rect`, `circle`, `arc`, `text`, `point`, `dim`, `hatch`, `fill`, `group`

Run `cadspec schema` for the complete reference with all attributes.

---

## Architecture Overview

```
.cf file (TOML)
    │
    ▼
┌─────────┐    ┌──────────┐    ┌─────────┐    ┌─────────┐
│ Parser  │───▶│ Resolver  │───▶│Compiler │───▶│ DXF Emit│
└─────────┘    └──────────┘    └─────────┘    └─────────┘
                     │
                     ▼
              ┌──────────┐    ┌─────────────┐
              │ Boundary │───▶│ Preview PNG │
              │ Resolver │    │ + JSON meta │
              └──────────┘    └─────────────┘
```

- **Parser** — TOML parsing with custom array-of-tables detection, primitive validation
- **Resolver** — Layer dependency resolution, coordinate validation, boundary detection
- **Compiler** — Entity compilation to DXF format, hatch generation, polyline closure
- **DXF Writer** — Direct DXF entity emission with proper layer/color/lineweight mapping
- **SVG Renderer** — Single vector backend: text, measured dims, hatches, highlights; PNG previews are resvg rasterizations of it

---

## Data Storage

| Data | Location | Format |
|------|----------|--------|
| Project files | `./` | TOML (`.cf` + `project.toml`) |
| Build output | `./output.dxf` | DXF |
| Preview output | `./preview.png`, `./preview.svg` | PNG / SVG |
| Preview metadata | `./preview.meta.json` | JSON |
| Language reference | `cadspec schema` (stdout) | Markdown |

---

## Tech Stack

| Concern | Crate |
|---|---|
| CLI parsing | `clap` (derive) |
| Error handling | `anyhow` |
| Serialization | `serde` + `toml` + `toml_edit` |
| DXF output | `dxf` |
| File watching | `notify` |
| PNG rasterization | `resvg` |

---

## License

MIT — see [LICENSE](LICENSE) for details.

---

An experiment of [UniverLab](https://github.com/UniverLab) — an open computational laboratory.
Made with ❤️ by [JheisonMB](https://github.com/JheisonMB)
