function __fish_using_command
    set cmd (commandline -opc)
    if [ (count $cmd) -eq (count $argv) ]
        for i in (seq (count $argv))
            if [ $cmd[$i] != $argv[$i] ]
                return 1
            end
        end
        return 0
    end
    return 1
end

complete -c feroxbuster -n "__fish_using_command feroxbuster" -s w -l wordlist -d 'Path to the wordlist'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -s u -l url -d 'The target URL(s) (required, unless --stdin used)'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -s t -l threads -d 'Number of concurrent threads (default: 50)'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -s d -l depth -d 'Maximum recursion depth, a depth of 0 is infinite recursion (default: 4)'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -s T -l timeout -d 'Number of seconds before a request times out (default: 7)'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -s p -l proxy -d 'Proxy to use for requests (ex: http(s)://host:port, socks5://host:port)'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -s P -l replay-proxy -d 'Send only unfiltered requests through a Replay Proxy, instead of all requests'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -s R -l replay-codes -d 'Status Codes to send through a Replay Proxy when found (default: --status-codes value)'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -s s -l status-codes -d 'Status Codes to include (allow list) (default: 200 204 301 302 307 308 401 403 405)'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -s o -l output -d 'Output file to write results to (use w/ --json for JSON entries)'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -l resume-from -d 'State file from which to resume a partially complete scan (ex. --resume-from ferox-1606586780.state)'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -l debug-log -d 'Output file to write log entries (use w/ --json for JSON entries)'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -s a -l user-agent -d 'Sets the User-Agent (default: feroxbuster/VERSION)'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -s x -l extensions -d 'File extension(s) to search for (ex: -x php -x pdf js)'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -s H -l headers -d 'Specify HTTP headers (ex: -H Header:val \'stuff: things\')'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -s Q -l query -d 'Specify URL query parameters (ex: -Q token=stuff -Q secret=key)'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -s S -l filter-size -d 'Filter out messages of a particular size (ex: -S 5120 -S 4927,1970)'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -s X -l filter-regex -d 'Filter out messages via regular expression matching on the response\'s body (ex: -X \'^ignore me$\')'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -s W -l filter-words -d 'Filter out messages of a particular word count (ex: -W 312 -W 91,82)'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -s N -l filter-lines -d 'Filter out messages of a particular line count (ex: -N 20 -N 31,30)'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -s C -l filter-status -d 'Filter out status codes (deny list) (ex: -C 200 -C 401)'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -s L -l scan-limit -d 'Limit total number of concurrent scans (default: 0, i.e. no limit)'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -s v -l verbosity -d 'Increase verbosity level (use -vv or more for greater effect. [CAUTION] 4 -v\'s is probably too much)'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -s q -l quiet -d 'Only print URLs; Don\'t print status codes, response size, running config, etc...'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -l json -d 'Emit JSON logs to --output and --debug-log instead of normal text'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -s D -l dont-filter -d 'Don\'t auto-filter wildcard responses'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -s r -l redirects -d 'Follow redirects'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -s k -l insecure -d 'Disables TLS certificate validation'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -s n -l no-recursion -d 'Do not scan recursively'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -s f -l add-slash -d 'Append / to each request'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -l stdin -d 'Read url(s) from STDIN'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -s e -l extract-links -d 'Extract links from response body (html, javascript, etc...); make new requests based on findings (default: false)'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -s h -l help -d 'Prints help information'
complete -c feroxbuster -n "__fish_using_command feroxbuster" -s V -l version -d 'Prints version information'
