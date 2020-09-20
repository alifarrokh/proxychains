# Proxychains
**Asynchronous Proxychains implementation using API Hooking written in Rust**

Currently Supported Proxies: `SOCKS5`

Proxychains Modes: `Random`, `Strict`, `Dynamic`

Note: The project is yet under development.

## Usage

1. Clone the project
```
$ git clone https://github.com/alifarrokh/proxychains.git
```

2. Build
```
$ cd proxychains
$ cargo build --release
```

3. Create `config.toml` (`config.sample.toml` is included)
```
mode = "Strict"
chain_len = 0

[[proxies]]
socket_addr = "127.0.0.1:1080"

[[proxies]]
socket_addr = "127.0.0.1:1081"
auth = ["username", "password"]
```

4. Run your app with `LD_PRELOAD`
```
$ LD_PRELOAD=path/to/proxychains/target/release/libproxychains.so yourapp --config "path/to/config.toml"
```
