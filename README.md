# Docker Monitor

A terminal-based Docker container monitoring tool built with Rust. This is a complete rewrite of [amir20/dtop](https://github.com/amir20/dtop) in Rust.

**Status**: Currently in active development. The project is working toward feature parity with the original dtop. Some features may be incomplete or missing.

Monitor CPU and memory usage of your Docker containers in real-time with a beautiful TUI interface.

## Features

- Real-time monitoring of Docker container metrics (CPU, Memory)
- Terminal User Interface (TUI) with keyboard navigation
- Support for local Docker daemon
- SSH support for remote Docker hosts
- Lightweight and fast
- Cross-platform (Linux, macOS, Windows)

## Installation

### Download Pre-built Binaries

Download the latest release for your platform from the [Releases](../../releases) page:

- **Linux x86_64** (Intel/AMD)
- **Linux ARM64** (Raspberry Pi, ARM servers)
- **macOS x86_64** (Intel Macs)
- **macOS ARM64** (Apple Silicon M1/M2/M3)

```bash
# Extract the archive
tar xzf docker-monitor-<platform>.tar.gz

# Make it executable
chmod +x docker-monitor

# Move to your PATH (optional)
sudo mv docker-monitor /usr/local/bin/
```

### Build from Source

Requires [Rust](https://rustup.rs/) 1.70 or later.

```bash
git clone https://github.com/yourusername/docker-monitor.git
cd docker-monitor
cargo build --release
```

The binary will be available at `target/release/docker-monitor`.

## Usage

### Monitor Local Docker Daemon

```bash
docker-monitor
```

or explicitly:

```bash
docker-monitor --host local
```

### Monitor Remote Docker Host via SSH

```bash
# Default SSH port (22)
docker-monitor --host ssh://user@remote-host

# Custom SSH port
docker-monitor --host ssh://user@remote-host:2222
```
## Requirements

- Docker daemon running locally or accessible via SSH
- For SSH connections: SSH access to the remote host with Docker permissions

## Architecture

Built with modern Rust async runtime and libraries:

- **[Tokio](https://tokio.rs/)** - Async runtime
- **[Ratatui](https://ratatui.rs/)** - Terminal UI framework
- **[Bollard](https://github.com/fussybeaver/bollard)** - Docker API client with SSH support
- **[Crossterm](https://github.com/crossterm-rs/crossterm)** - Cross-platform terminal manipulation
- **[Clap](https://github.com/clap-rs/clap)** - Command-line argument parsing

## Development

### Running in Development

```bash
cargo run
```

### Running with Arguments

```bash
# Local Docker
cargo run -- --host local

# Remote Docker via SSH
cargo run -- --host ssh://user@host
```

### Building for Release

```bash
cargo build --release
```

## CI/CD

This project uses GitHub Actions for continuous integration and deployment:

- **Pull Requests**: Automatic builds for all platforms with artifacts attached to PR comments
- **Releases**: Tag-based releases that automatically build and publish binaries for all platforms

This will trigger the release workflow and create a GitHub release with binaries for all supported platforms.

## License

[MIT License](LICENSE) or [Apache 2.0](LICENSE-APACHE) (your choice)

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## Troubleshooting

### Cannot connect to Docker daemon

Ensure Docker is running and your user has permissions to access the Docker socket:

```bash
# Linux: Add your user to the docker group
sudo usermod -aG docker $USER
# Log out and back in for changes to take effect
```

### SSH connection fails

Ensure:
- SSH access is configured correctly
- Your SSH key is loaded (`ssh-add`)
- The remote user has Docker permissions
- The Docker daemon is running on the remote host

## Roadmap

- [ ] Container logs viewer
- [ ] Container start/stop controls
- [ ] Network and disk I/O metrics
- [ ] Historical data graphs
- [ ] Support for Docker Compose projects
- [ ] Configuration file support
- [ ] Custom refresh intervals

## Acknowledgments

Built with excellent open-source libraries from the Rust ecosystem.
