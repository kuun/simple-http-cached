# Simple HTTP Cached Proxy

A lightweight HTTP/HTTPS proxy server with local file caching capabilities, written in Rust.

## Features

- Supports both HTTP and HTTPS proxying
- Local file caching of responses
- Handles redirects with cache support
- Skip caching for 5xx server errors
- Configurable target server and local listening address

## Installation

1. Make sure you have Rust and Cargo installed
2. Clone the repository:
```bash
git clone https://github.com/yourusername/simple-http-cached.git
cd simple-http-cached
```
3. Create the cache directory:
```bash
sudo mkdir -p /var/lib/simple_http_cache
sudo chmod 777 /var/lib/simple_http_cache
```

## Usage

Run the proxy server with default settings:

```bash
cargo run
```

### Command Line Options

- `-t, --target-server`: Target server to proxy requests to (default: "snapshot.debian.org")
- `-l, --listen-addr`: Address to listen on (default: "127.0.0.1:8100")
- `-p, --protocol`: Protocol to use, http or https (default: "https")

Example with custom settings:

```bash
cargo run -- --target-server example.com --listen-addr 127.0.0.1:8080 --protocol http
```

## How it Works

1. The proxy listens for incoming HTTP requests
2. For each request:
   - Checks if the response is already cached
   - If cached, returns the cached response
   - If not cached, forwards the request to the target server
   - Caches the response (except for 5xx errors)
   - Handles redirects by caching the redirect location

## Cache Location

Cached responses are stored in `/var/lib/simple_http_cache/`.

## Requirements

- Rust 1.75 or later
- Linux operating system
- Write permissions for `/var/lib/simple_http_cache/`

## License

MIT License