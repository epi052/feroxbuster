complete -c feroxbuster -s u -l url -d 'The target URL (required, unless [--stdin || --resume-from || --request-file] used)' -r -f
complete -c feroxbuster -l resume-from -d 'State file from which to resume a partially complete scan (ex. --resume-from ferox-1606586780.state)' -r -F
complete -c feroxbuster -l request-file -d 'Raw HTTP request file to use as a template for all requests' -r -F
complete -c feroxbuster -l data-urlencoded -d 'Set -H \'Content-Type: application/x-www-form-urlencoded\', --data to <data-urlencoded> (supports @file) and -m to POST' -r
complete -c feroxbuster -l data-json -d 'Set -H \'Content-Type: application/json\', --data to <data-json> (supports @file) and -m to POST' -r
complete -c feroxbuster -s p -l proxy -d 'Proxy to use for requests (ex: http(s)://host:port, socks5(h)://host:port)' -r -f
complete -c feroxbuster -s P -l replay-proxy -d 'Send only unfiltered requests through a Replay Proxy, instead of all requests' -r -f
complete -c feroxbuster -s R -l replay-codes -d 'Status Codes to send through a Replay Proxy when found (default: --status-codes value)' -r
complete -c feroxbuster -s a -l user-agent -d 'Sets the User-Agent (default: feroxbuster/2.13.1)' -r
complete -c feroxbuster -s x -l extensions -d 'File extension(s) to search for (ex: -x php -x pdf js); reads values (newline-separated) from file if input starts with an @ (ex: @ext.txt)' -r
complete -c feroxbuster -s m -l methods -d 'Which HTTP request method(s) should be sent (default: GET)' -r
complete -c feroxbuster -l data -d 'Request\'s Body; can read data from a file if input starts with an @ (ex: @post.bin)' -r
complete -c feroxbuster -s H -l headers -d 'Specify HTTP headers to be used in each request (ex: -H Header:val -H \'stuff: things\')' -r
complete -c feroxbuster -s b -l cookies -d 'Specify HTTP cookies to be used in each request (ex: -b stuff=things)' -r
complete -c feroxbuster -s Q -l query -d 'Request\'s URL query parameters (ex: -Q token=stuff -Q secret=key)' -r
complete -c feroxbuster -l protocol -d 'Specify the protocol to use when targeting via --request-file or --url with domain only (default: https)' -r
complete -c feroxbuster -l dont-scan -d 'URL(s) or Regex Pattern(s) to exclude from recursion/scans' -r
complete -c feroxbuster -l scope -d 'Additional domains/URLs to consider in-scope for scanning (in addition to current domain)' -r
complete -c feroxbuster -s S -l filter-size -d 'Filter out messages of a particular size (ex: -S 5120 -S 4927,1970)' -r
complete -c feroxbuster -s X -l filter-regex -d 'Filter out messages via regular expression matching on the response\'s body/headers (ex: -X \'^ignore me$\')' -r
complete -c feroxbuster -s W -l filter-words -d 'Filter out messages of a particular word count (ex: -W 312 -W 91,82)' -r
complete -c feroxbuster -s N -l filter-lines -d 'Filter out messages of a particular line count (ex: -N 20 -N 31,30)' -r
complete -c feroxbuster -s C -l filter-status -d 'Filter out status codes (deny list) (ex: -C 200 -C 401)' -r
complete -c feroxbuster -l filter-similar-to -d 'Filter out pages that are similar to the given page (ex. --filter-similar-to http://site.xyz/soft404)' -r -f
complete -c feroxbuster -s s -l status-codes -d 'Status Codes to include (allow list) (default: All Status Codes)' -r
complete -c feroxbuster -s T -l timeout -d 'Number of seconds before a client\'s request times out (default: 7)' -r
complete -c feroxbuster -l server-certs -d 'Add custom root certificate(s) for servers with unknown certificates' -r -F
complete -c feroxbuster -l client-cert -d 'Add a PEM encoded certificate for mutual authentication (mTLS)' -r -F
complete -c feroxbuster -l client-key -d 'Add a PEM encoded private key for mutual authentication (mTLS)' -r -F
complete -c feroxbuster -s t -l threads -d 'Number of concurrent threads (default: 50)' -r
complete -c feroxbuster -s d -l depth -d 'Maximum recursion depth, a depth of 0 is infinite recursion (default: 4)' -r
complete -c feroxbuster -s L -l scan-limit -d 'Limit total number of concurrent scans (default: 0, i.e. no limit)' -r
complete -c feroxbuster -l parallel -d 'Run parallel feroxbuster instances (one child process per url passed via stdin)' -r
complete -c feroxbuster -l rate-limit -d 'Limit number of requests per second (per directory) (default: 0, i.e. no limit)' -r
complete -c feroxbuster -l response-size-limit -d 'Limit size of response body to read in bytes (default: 4MB)' -r
complete -c feroxbuster -l time-limit -d 'Limit total run time of all scans (ex: --time-limit 10m)' -r
complete -c feroxbuster -s w -l wordlist -d 'Path or URL of the wordlist' -r -F
complete -c feroxbuster -s B -l collect-backups -d 'Automatically request likely backup extensions for "found" urls (default: ~, .bak, .bak2, .old, .1)' -r
complete -c feroxbuster -s I -l dont-collect -d 'File extension(s) to Ignore while collecting extensions (only used with --collect-extensions)' -r
complete -c feroxbuster -s o -l output -d 'Output file to write results to (use w/ --json for JSON entries)' -r -F
complete -c feroxbuster -l debug-log -d 'Output file to write log entries (use w/ --json for JSON entries)' -r -F
complete -c feroxbuster -l limit-bars -d 'Number of directory scan bars to show at any given time (default: no limit)' -r
complete -c feroxbuster -l stdin -d 'Read url(s) from STDIN'
complete -c feroxbuster -l burp -d 'Set --proxy to http://127.0.0.1:8080 and set --insecure to true'
complete -c feroxbuster -l burp-replay -d 'Set --replay-proxy to http://127.0.0.1:8080 and set --insecure to true'
complete -c feroxbuster -l smart -d 'Set --auto-tune, --collect-words, and --collect-backups to true'
complete -c feroxbuster -l thorough -d 'Use the same settings as --smart and set --collect-extensions and --scan-dir-listings to true'
complete -c feroxbuster -s A -l random-agent -d 'Use a random User-Agent'
complete -c feroxbuster -s f -l add-slash -d 'Append / to each request\'s URL'
complete -c feroxbuster -l unique -d 'Only show unique responses'
complete -c feroxbuster -s r -l redirects -d 'Allow client to follow redirects'
complete -c feroxbuster -s k -l insecure -d 'Disables TLS certificate validation in the client'
complete -c feroxbuster -s n -l no-recursion -d 'Do not scan recursively'
complete -c feroxbuster -l force-recursion -d 'Force recursion attempts on all \'found\' endpoints (still respects recursion depth)'
complete -c feroxbuster -s e -l extract-links -d 'Extract links from response body (html, javascript, etc...); make new requests based on findings (default: true)'
complete -c feroxbuster -l dont-extract-links -d 'Don\'t extract links from response body (html, javascript, etc...)'
complete -c feroxbuster -l auto-tune -d 'Automatically lower scan rate when an excessive amount of errors are encountered'
complete -c feroxbuster -l auto-bail -d 'Automatically stop scanning when an excessive amount of errors are encountered'
complete -c feroxbuster -s D -l dont-filter -d 'Don\'t auto-filter wildcard responses'
complete -c feroxbuster -s E -l collect-extensions -d 'Automatically discover extensions and add them to --extensions (unless they\'re in --dont-collect)'
complete -c feroxbuster -s g -l collect-words -d 'Automatically discover important words from within responses and add them to the wordlist'
complete -c feroxbuster -l scan-dir-listings -d 'Force scans to recurse into directory listings'
complete -c feroxbuster -s v -l verbosity -d 'Increase verbosity level (use -vv or more for greater effect. [CAUTION] 4 -v\'s is probably too much)'
complete -c feroxbuster -l silent -d 'Only print URLs (or JSON w/ --json) + turn off logging (good for piping a list of urls to other commands)'
complete -c feroxbuster -s q -l quiet -d 'Hide progress bars and banner (good for tmux windows w/ notifications)'
complete -c feroxbuster -l json -d 'Emit JSON logs to --output and --debug-log instead of normal text'
complete -c feroxbuster -l no-state -d 'Disable state output file (*.state)'
complete -c feroxbuster -s U -l update -d 'Update feroxbuster to the latest version'
complete -c feroxbuster -s h -l help -d 'Print help (see more with \'--help\')'
complete -c feroxbuster -s V -l version -d 'Print version'
