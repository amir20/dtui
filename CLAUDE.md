# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Docker Monitor is a terminal-based Docker container monitoring tool built with Rust. It provides real-time CPU and memory metrics for Docker containers through a TUI interface, with support for both local and remote (SSH) Docker daemons.

## Build & Run Commands

```bash
# Development
cargo run                                    # Run with local Docker daemon
cargo run -- --host ssh://user@host         # Run with remote Docker host

# Production build
cargo build --release

# The binary will be at target/release/docker-monitor
```

## Architecture

The application follows an **event-driven architecture** with three main async/threaded components communicating via a single mpsc channel (`AppEvent`):

### Core Components

1. **Main Event Loop** (`main.rs::run_event_loop`)
   - Receives events from the channel
   - Maintains container state in `HashMap<String, ContainerInfo>`
   - Renders UI at 500ms intervals using Ratatui
   - Single source of truth for container data

2. **Container Manager** (`docker.rs::container_manager`)
   - Async task that manages Docker API interactions
   - Fetches initial container list on startup
   - Subscribes to Docker events (start/stop/die)
   - Spawns individual stats stream tasks per container
   - Each container gets its own async task running `stream_container_stats`

3. **Keyboard Worker** (`input.rs::keyboard_worker`)
   - Blocking thread that polls keyboard input every 200ms
   - Sends `AppEvent::Quit` on 'q' press
   - Separate thread because crossterm's event polling is blocking

### Event Flow

```
Docker API → container_manager → AppEvent → Main Loop → UI Render
Keyboard   → keyboard_worker   → AppEvent → Main Loop → Exit
```

### Event Types (`types.rs::AppEvent`)

- `ContainerUpdate(id, ContainerInfo)` - Stats update from a container
- `ContainerRemoved(id)` - Container stopped/died
- `InitialContainerList(Vec)` - Batch of containers on startup or new container started
- `Quit` - User pressed 'q'

### Docker Connection

The `connect_docker()` function in `main.rs` handles two connection modes:
- `--host local`: Uses local Docker socket
- `--host ssh://user@host[:port]`: Connects via SSH (requires Bollard SSH feature)

### Stats Calculation

CPU and memory percentages are calculated in `docker.rs`:
- **CPU**: Delta between current and previous CPU usage, normalized by system CPU delta and CPU count
- **Memory**: Current usage divided by limit, expressed as percentage

### UI Rendering

The UI (`ui.rs`) uses pre-allocated styles to avoid per-frame allocations. Color coding:
- Green: 0-50%
- Yellow: 50-80%
- Red: >80%

## CI/CD Workflows

### Release Workflow (`.github/workflows/release.yml`)
- Triggers on version tags (e.g., `v0.1.0`)
- Builds for 4 platforms using matrix strategy:
  - Linux x86_64 (native cargo)
  - Linux ARM64 (cross tool)
  - macOS x86_64 (native cargo on macOS runner)
  - macOS ARM64 (native cargo on macOS runner)
- Uses `softprops/action-gh-release@v2` to create GitHub releases

### PR Build Workflow (`.github/workflows/pr-build.yml`)
- Same build matrix as release workflow
- Posts comment on PR with artifact download links
- Updates existing comment on subsequent pushes

**Note**: `cross` is only used for Linux ARM64. macOS builds require native runners because Docker can't containerize macOS.

## Key Dependencies

- **Tokio**: Async runtime for Docker API and event handling
- **Bollard**: Docker API client with SSH support
- **Ratatui**: Terminal UI framework
- **Crossterm**: Cross-platform terminal manipulation
- **Clap**: CLI argument parsing

## Performance Considerations

- UI refresh rate is throttled to 500ms to reduce CPU usage
- Container stats streams run independently per container
- Keyboard polling is 200ms to balance responsiveness and CPU
- Styles are pre-allocated in `UiStyles::default()` to avoid allocations during rendering
- Container references (not clones) are used when building UI rows
