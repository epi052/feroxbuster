<h1 align="center">
  <br>
  <a href="https://github.com/epi052/feroxbuster"><img src="img/logo/default-cropped.png" alt="feroxbuster"></a>
  <br>
</h1>

<h4 align="center">A simple, fast, recursive content discovery tool written in Rust</h4>

<p align="center">
  <a href="https://github.com/epi052/feroxbuster/actions?query=workflow%3A%22CI+Pipeline%22">
    <img src="https://img.shields.io/github/workflow/status/epi052/feroxbuster/CI%20Pipeline/master?logo=github">
  </a>
  
  ✨
  
  <a href="https://github.com/epi052/feroxbuster/releases">
    <img src="https://img.shields.io/github/downloads/epi052/feroxbuster/total?label=downloads&logo=github&color=inactive" alt="github downloads">
  </a>
  
  ✨
  
  <a href="https://github.com/epi052/feroxbuster/commits/master">
    <img src="https://img.shields.io/github/last-commit/epi052/feroxbuster?logo=github">
  </a>
  
  ✨
  
  <a href="https://crates.io/crates/feroxbuster">
    <img src="https://img.shields.io/crates/v/feroxbuster?color=blue&label=version&logo=rust">
  </a>

  ✨

  <a href="https://crates.io/crates/feroxbuster">
    <img src="https://img.shields.io/crates/d/feroxbuster?label=downloads&logo=rust&color=inactive">
  </a>
  
  ✨
  
  <a href="https://codecov.io/gh/epi052/feroxbuster">
    <img src="https://codecov.io/gh/epi052/feroxbuster/branch/master/graph/badge.svg" />
  </a>
</p>

![demo](img/demo.gif)

<p align="center">
  <a href="https://github.com/epi052/feroxbuster/releases">Releases</a> •
  <a href="#-example-usage">Example Usage</a> •
  <a href="https://github.com/epi052/feroxbuster/blob/master/CONTRIBUTING.md">Contributing</a> •
  <a href="https://docs.rs/feroxbuster/latest/feroxbuster/">Documentation</a>
</p>

## 😕 What the heck is a ferox anyway?

Ferox is short for Ferric Oxide. Ferric Oxide, simply put, is rust.  The name rustbuster was taken, so I decided on a variation.  🤷	

📖 Table of Contents
-----------------
- [Downloads](#-downloads)
- [Installation](#-installation)
    - [Download a Release](#download-a-release)
    - [Cargo Install](#cargo-install)
    - [apt Install](#apt-install)
- [Configuration](#-configuration)
    - [Default Values](#default-values)
    - [ferox-config.toml](#ferox-configtoml)
    - [Command Line Parsing](#command-line-parsing)
- [Example Usage](#-example-usage)
    - [Multiple Values](#multiple-values)
    - [Include Headers](#include-headers)
    - [IPv6, Non-recursive scan with INFO logging enabled](#ipv6-non-recursive-scan-with-info-level-logging-enabled)
    - [Read urls from STDIN; pipe only resulting urls out to another tool](#read-urls-from-stdin-pipe-only-resulting-urls-out-to-another-tool)
    - [Proxy traffic through Burp](#proxy-traffic-through-burp)
    - [Proxy traffic through a SOCKS proxy](#proxy-traffic-through-a-socks-proxy)
    - [Pass auth token via query parameter](#pass-auth-token-via-query-parameter)
- [Comparison w/ Similar Tools](#-comparison-w-similar-tools)

## 💿 Installation

### Download a Release

Releases for multiple architectures can be found in the [Releases](https://github.com/epi052/feroxbuster/releases) section.  Builds for the following systems are currently supported:

- Linux x86
- Linux x86_64
- MacOS x86_64
- Windows x86
- Windows x86_64

### Cargo Install

`feroxbuster` is published on crates.io, making it easy to install if you already have rust installed on your system.

```
cargo install feroxbuster
```

### apt Install

Head to the [Releases](https://github.com/epi052/feroxbuster/releases) section and download `feroxbuster_amd64.deb`.  After that, use your favorite package manager to install the .deb.

```
sudo apt install ./feroxbuster_amd64.deb
```

## ⚙️ Configuration
### Default Values
Configuration begins with with the following built-in default values baked into the binary:

- timeout: `7` seconds
- follow redirects: `false`
- wordlist: `/usr/share/seclists/Discovery/Web-Content/raft-medium-directories.txt`
- threads: `50`
- verbosity: `0` (no logging enabled)
- statuscodes: `200 204 301 302 307 308 401 403 405`
- useragent: `feroxbuster/VERSION`
- recursion depth: `4`
- auto-filter wildcards - `true`
- output: `stdout`

### ferox-config.toml
After setting built-in default values, any values defined in a `ferox-config.toml` config file will override the
built-in defaults.  

`feroxbuster` searches for `ferox-config.toml` in the following locations (in the order shown):
- `/etc/feroxbuster/` (global)
- `CONFIG_DIR/ferxobuster/` (per-user)
- The same directory as the `feroxbuster` executable (per-user)
- The user's current working directory (per-target)

If more than one valid configuration file is found, each one overwrites the values found previously.  

If no configuration file is found, nothing happens at this stage.

As an example, let's say that we prefer to use a different wordlist as our default when scanning; we can
set the `wordlist` value in the config file to override the baked-in default.

Notes of interest:
- it's ok to only specify values you want to change without specifying anything else
- variable names in `ferox-config.toml` must match their command-line counterpart

```toml
# ferox-config.toml

wordlist = "/wordlists/jhaddix/all.txt"
```

A pre-made configuration file with examples of all available settings can be found in `ferox-config.toml.example`.
```toml
# ferox-config.toml
# Example configuration for feroxbuster
#
# If you wish to provide persistent settings to feroxbuster, rename this file to ferox-config.toml and make sure
# it resides in the same directory as the feroxbuster binary.
#
# After that, uncomment any line to override the default value provided by the binary itself.
#
# Any setting used here can be overridden by the corresponding command line option/argument
#
# wordlist = "/wordlists/jhaddix/all.txt"
# statuscodes = [200, 500]
# threads = 1
# timeout = 5
# proxy = "http://127.0.0.1:8080"
# verbosity = 1
# quiet = true
# output = "/targets/ellingson_mineral_company/gibson.txt"
# useragent = "Mozilla/5.0 (Windows NT 6.1; Win64; x64; rv:47.0) Gecko/20100101 Firefox/47.0"
# redirects = true
# insecure = true
# extensions = ["php", "html"]
# norecursion = true
# addslash = true
# stdin = true
# dontfilter = true
# depth = 1
# sizefilters = [5174]
# queries = [["name","value"], ["rick", "astley"]]

# headers can be specified on multiple lines or as an inline table
#
# inline example
# headers = {"stuff" = "things"}
#
# multi-line example
#   note: if multi-line is used, all key/value pairs under it belong to the headers table until the next table
#         is found or the end of the file is reached
#
# [headers]
# stuff = "things"
# more = "headers"
```

### Command Line Parsing
Finally, after parsing the available config file, any options/arguments given on the commandline will override any values that were set as a built-in or config-file value.

```
USAGE:
    feroxbuster [FLAGS] [OPTIONS] --url <URL>...

FLAGS:
    -f, --addslash       Append / to each request
    -D, --dontfilter     Don't auto-filter wildcard responses
    -h, --help           Prints help information
    -k, --insecure       Disables TLS certificate validation
    -n, --norecursion    Do not scan recursively
    -q, --quiet          Only print URLs; Don't print status codes, response size, running config, etc...
    -r, --redirects      Follow redirects
        --stdin          Read url(s) from STDIN
    -V, --version        Prints version information
    -v, --verbosity      Increase verbosity level (use -vv or more for greater effect)

OPTIONS:
    -d, --depth <RECURSION_DEPTH>           Maximum recursion depth, a depth of 0 is infinite recursion (default: 4)
    -x, --extensions <FILE_EXTENSION>...    File extension(s) to search for (ex: -x php -x pdf js)
    -H, --headers <HEADER>...               Specify HTTP headers (ex: -H Header:val 'stuff: things')
    -o, --output <FILE>                     Output file to write results to (default: stdout)
    -p, --proxy <PROXY>                     Proxy to use for requests (ex: http(s)://host:port, socks5://host:port)
    -Q, --query <QUERY>...                  Specify URL query parameters (ex: -Q token=stuff -Q secret=key)
    -S, --sizefilter <SIZE>...              Filter out messages of a particular size (ex: -S 5120 -S 4927,1970)
    -s, --statuscodes <STATUS_CODE>...      Status Codes of interest (default: 200 204 301 302 307 308 401 403 405)
    -t, --threads <THREADS>                 Number of concurrent threads (default: 50)
    -T, --timeout <SECONDS>                 Number of seconds before a request times out (default: 7)
    -u, --url <URL>...                      The target URL(s) (required, unless --stdin used)
    -a, --useragent <USER_AGENT>            Sets the User-Agent (default: feroxbuster/VERSION)
    -w, --wordlist <FILE>                   Path to the wordlist
```

## 🧰 Example Usage

### Multiple Values

Options that take multiple values are very flexible.  Consider the following ways of specifying extensions:

```
./feroxbuster -u http://127.1 -x pdf -x js,html -x php txt json,docx
```

The command above adds .pdf, .js, .html, .php, .txt, .json, and .docx to each url

All of the methods above (multiple flags, space separated, comma separated, etc...) are valid and interchangeable.  The same goes for urls, headers, status codes, queries, and size filters.

### Include Headers

```
./feroxbuster -u http://127.1 -H Accept:application/json "Authorization: Bearer {token}"
```

### IPv6, non-recursive scan with INFO-level logging enabled

```
./feroxbuster -u http://[::1] --norecursion -vv
```

### Read urls from STDIN; pipe only resulting urls out to another tool

```
cat targets | ./feroxbuster --stdin --quiet -s 200 301 302 --redirects -x js | fff -s 200 -o js-files
```

### Proxy traffic through Burp

```
./feroxbuster -u http://127.1 --insecure --proxy http://127.0.0.1:8080
```

### Proxy traffic through a SOCKS proxy

```
./feroxbuster -u http://127.1 --proxy socks5://127.0.0.1:9050
```

### Pass auth token via query parameter

```
./feroxbuster -u http://127.1 --query token=0123456789ABCDEF
```


## 🧐 Comparison w/ Similar Tools

There are quite a few similar tools for forced browsing/content discovery.  Burp Suite Pro, Dirb, Dirbuster, etc... 
However, in my opinion, there are two that set the standard: [gobuster](https://github.com/OJ/gobuster) and 
[ffuf](https://github.com/ffuf/ffuf).  Both are mature, feature-rich, and all-around incredible tools to use.

So, why would you ever want to use feroxbuster over ffuf/gobuster?  In most cases, you probably won't.  ffuf in particular
can do the vast majority of things that feroxbuster can, while still offering boatloads more functionality.  Here are
a few of the use-cases in which feroxbuster may be a better fit:

- You want a **simple** tool usage experience
- You want to be able to run your content discovery as part of some crazy 12 command unix **pipeline extravaganza**
- You want to scan through a **SOCKS** proxy
- You want **auto-filtering** of Wildcard responses by default
- You want **recursion** along with some other thing mentioned above (ffuf also does recursion)
- You want a **configuration file** option for overriding built-in default values for your scans

|                                                     | feroxbuster | gobuster | ffuf |
|-----------------------------------------------------|---|---|---|
| fast                                                | ✔ | ✔ | ✔ |
| easy to use                                         | ✔ | ✔ |   |
| blacklist status codes (in addition to whitelist)   |   | ✔ | ✔ |
| allows recursion                                    | ✔ |   | ✔ |
| can specify query parameters                        | ✔ |   | ✔ |
| SOCKS proxy support                                 | ✔ |   |   |
| multiple target scan (via stdin or multiple -u)     | ✔ |   |   |
| configuration file for default value override       | ✔ |   | ✔ |
| can accept urls via STDIN as part of a pipeline     | ✔ |   |   |
| can accept wordlists via STDIN                      |   | ✔ |   |
| filter by response size                             | ✔ |   | ✔ |
| auto-filter wildcard responses                      | ✔ |   | ✔ |
| performs other scans (vhost, dns, etc)              |   | ✔ | ✔ |
| time delay / rate limiting                          |   | ✔ | ✔ |
| **huge** number of other options                    |   |   | ✔ |

Of note, there's another written-in-rust content discovery tool, [rustbuster](https://github.com/phra/rustbuster). I 
came across rustbuster when I was naming my tool (😢). I don't have any experience using it, but it appears to 
be able to do POST requests with an HTTP body, has SOCKS support, and has an 8.3 shortname scanner (in addition to vhost
dns, directory, etc...).  In short, it definitely looks interesting and may be what you're looking for as it has some 
capability I haven't seen in similar tools.  
