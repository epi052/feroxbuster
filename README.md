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

### ferox-config.toml
After setting built-in default values, any values defined in a `ferox-config.toml` config file will override the
built-in defaults.  If `ferox-config.toml` is not found in the **same directory** as `feroxbuster`, nothing happens at this stage. 

For example, say that we prefer to use a different wordlist as our default when scanning; we can
set the `wordlist` value in the config file to override the baked-in default.

Notes of interest:
- it's ok to only specify values you want to change without specifying anything else
- variable names in `ferox-config.toml` must match their command-line counterpart

```toml
# ferox-config.toml

wordlist = "/wordlists/jhaddix/all.txt"
```

Example usage of all available settings in ferox-config.toml (can also be found in `ferox-config.toml.example`)
```toml
# ferox-config.toml

wordlist = "/wordlists/jhaddix/all.txt"
statuscodes = [200, 403]
threads = 40
timeout = 5
proxy = "http://127.0.0.1:8080"
verbosity = 1
quiet = true
verbosity = 1
output = "/some/output/file/path"
redirects = true
insecure = true
extensions = ["php", "html"]
headers = {"Accept" = "application/json"}
norecursion = true
addslash = true
stdin = true
```

### Command Line Parsing
Finally, any options/arguments given on the commandline will override both built-in and
config-file specified values.

```
USAGE:
    feroxbuster [FLAGS] [OPTIONS] --url <URL>

FLAGS:
    -f, --addslash       Append / to each request (default: false)
    -h, --help           Prints help information
    -k, --insecure       Disables TLS certificate validation (default: false)
    -n, --norecursion    Do not scan recursively (default: scan recursively)
    -q, --quiet          Only print URLs; Don't print status codes, response size, running config, etc...
    -r, --redirects      Follow redirects (default: false)
    -V, --version        Prints version information
    -v, --verbosity      Increase verbosity level (use -vv or more for greater effect)

OPTIONS:
    -x, --extensions <FILE_EXTENSION>...    File extension(s) to search for (accepts multi-flag and space or comma
                                            -delimited: -x php -x pdf js)
    -H, --headers <HEADER>...               Specify HTTP headers, -H Header:val 'stuff: things' -H 'MoHeaders: movals'
    -o, --output <FILE>                     Output file to write results to (default: stdout)
    -p, --proxy <proxy>                     Proxy to use for requests (ex: http(s)://host:port, socks5://host:port)
    -s, --statuscodes <STATUS_CODE>...      Status Codes of interest (default: 200 204 301 302 307 308 401 403 405)
    -t, --threads <THREADS>                 Number of concurrent threads (default: 50)
    -T, --timeout <SECONDS>                 Number of seconds before a request times out (default: 7)
    -u, --url <URL>                         The target URL
    -a, --useragent <USER_AGENT>            Sets the User-Agent (default: feroxbuster/VERSION)
    -w, --wordlist <FILE>                   Path to the wordlist

NOTE:
    Options that take multiple values are very flexible.  Consider the following ways of specifying
    extensions:
        ./feroxbuster -u http://127.1 -x pdf -x js,html -x php txt json,docx

    All of the methods above are valid and interchangeable.  The same goes for headers and status
    codes.

EXAMPLES:
    Multiple headers:
        ./feroxbuster -u http://127.1 -H Accept:application/json "Authorization: Bearer {token}"

    IPv6, non-recursive scan with INFO-level logging enabled:
        ./feroxbuster -u http://[::1] --norecursion -vv

    Read urls from STDIN; pipe only resulting urls out to another tool
        cat targets | ./feroxbuster -q -s 200 301 302 --redirects -x js | fff -s 200 -o js-files

    Ludicrous speed... go!
        ./feroxbuster -u http://127.1 -t 200
```

## Comparison w/ Similar Tools

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

|                                                     | feroxbuster        | gobuster           | ffuf               |
|-----------------------------------------------------|--------------------|--------------------|--------------------|
| fast                                                | :heavy_check_mark: | :heavy_check_mark: | :heavy_check_mark: |
| easy to use                                         | :heavy_check_mark: | :heavy_check_mark: |                    |
| blacklist status codes (in addition to whitelist)   |                    | :heavy_check_mark: | :heavy_check_mark: |
| allows recursion                                    | :heavy_check_mark: |                    | :heavy_check_mark: |
| can specify query parameters                        | :heavy_check_mark: |                    | :heavy_check_mark: |
| SOCKS proxy support                                 | :heavy_check_mark: |                    |                    |
| multiple target scan (via stdin or multiple -u)     | :heavy_check_mark: |                    |                    |
| configuration file for default value override       | :heavy_check_mark: |                    |                    |
| can accept urls via STDIN as part of a pipeline     | :heavy_check_mark: |                    |                    |
| can accept wordlists via STDIN                      |                    | :heavy_check_mark: |                    |
| filter by response size                             | :heavy_check_mark: |                    | :heavy_check_mark: |
| auto-filter wildcard responses                      | :heavy_check_mark: |                    | :heavy_check_mark: |
| performs other scans (vhost, dns, etc)              |                    | :heavy_check_mark: | :heavy_check_mark: |
| **huge** number of other options                    |                    |                    | :heavy_check_mark: |

Of note, there's another written-in-rust content discovery tool, [rustbuster](https://github.com/phra/rustbuster). I 
came across rustbuster when I was naming my tool (:cry:). I don't have any experience using it, but it appears to 
be able to do POST requests with an HTTP body, has SOCKS support, and has an 8.3 shortname scanner (in addition to vhost
dns, directory, etc...).  In short, it definitely looks interesting and may be what you're looking for as it has some 
capability I haven't seen in other tools.  
