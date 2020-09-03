# feroxbuster

`feroxbuster` is a fast, simple, recursive content discovery tool written in Rust.

Table of Contents
-----------------
- [Downloads](#downloads)
- [Installation](#installation)
- [Configuration](#configuration)
- [Comparison w/ Similar Tools](#comparison-w-similar-tools)

## Downloads
There are pre-built binaries for the following systems:

- [Linux x86](https://github.com/epi052/feroxbuster/releases/latest/download/x86-linux-feroxbuster.zip)
- [Linux x86_64](https://github.com/epi052/feroxbuster/releases/latest/download/x86_64-linux-feroxbuster.zip)
- [MacOS x86_64](https://github.com/epi052/feroxbuster/releases/latest/download/x86_64-macos-feroxbuster.zip)
- [Windows x86](https://github.com/epi052/feroxbuster/releases/latest/download/x86-windows-feroxbuster.exe.zip)
- [Windows x86_64](https://github.com/epi052/feroxbuster/releases/latest/download/x86_64-windows-feroxbuster.exe.zip)

## Installation
## Configuration
### Defaults
Configuration begins with with the following built-in default values baked into the binary:

- timeout: `7` seconds
- follow redirects: `false`
- wordlist: `/usr/share/seclists/Discovery/Web-Content/raft-medium-directories.txt`
- threads: `50`
- verbosity: `0` (no logging enabled)
- statuscodes: `200 204 301 302 307 308 401 403 405`

### feroxbuster.toml
After setting built-in default values, any values defined in a `feroxbuster.toml` config file will override the
built-in defaults.  If `feroxbuster.toml` is not found in the current directory, nothing happens at this stage. 

For example, say that we prefer to use a different wordlist as our default when scanning; we can
set the `wordlist` value in the config file to override the baked-in default.

Notes of interest:
- it's ok to only specify values you want to change without specifying anything else
- variable names in feroxbuster.toml must match their command-line counterpart

```toml
# feroxbuster.toml

wordlist = "/wordlists/jhaddix/all.txt"
```

Example usage of all available settings in feroxbuster.toml (can also be found in `feroxbuster.toml.example`)
```toml
# feroxbuster.toml

wordlist = "/wordlists/jhaddix/all.txt"
statuscodes = [200, 403]
threads = 40
timeout = 5
proxy = "http://127.0.0.1:8080"
verbosity = 1
quiet = true
verbosity = 1
output = "/some/output/file/path"
follow_redirects = true
insecure = true
extensions = ["php", "html"]
headers = {"Accept" = "application/json"}
```

### Command Line Parsing
Finally, any options/arguments given on the commandline will override both built-in and
config-file specified values.

```
USAGE:
    feroxbuster [FLAGS] [OPTIONS] --url <URL>

FLAGS:
    -h, --help         Prints help information
    -V, --version      Prints version information
    -v, --verbosity    Increase verbosity level (use -vv or more for greater effect)

OPTIONS:
    -p, --proxy <proxy>                   Proxy to use for requests (ex: http(s)://host:port, socks5://host:port)
    -s, --statuscodes <STATUS_CODE>...    Status Codes of interest (default: 200 204 301 302 307 308 401 403 405)
    -t, --threads <THREADS>               Number of concurrent threads (default: 50)
    -T, --timeout <SECONDS>               Number of seconds before a request times out (default: 7)
    -u, --url <URL>                       The target URL
    -w, --wordlist <FILE>                 Path to the wordlist

```

## Comparison w/ Similar Tools
### How does `feroxbuster` compare to [gobuster](https://github.com/OJ/gobuster)
### How does `feroxbuster` compare to [ffuf](https://github.com/ffuf/ffuf)
### How does `feroxbuster` compare to [rustbuster](https://github.com/phra/rustbuster)
