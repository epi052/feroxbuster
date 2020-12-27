
using namespace System.Management.Automation
using namespace System.Management.Automation.Language

Register-ArgumentCompleter -Native -CommandName 'feroxbuster' -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)

    $commandElements = $commandAst.CommandElements
    $command = @(
        'feroxbuster'
        for ($i = 1; $i -lt $commandElements.Count; $i++) {
            $element = $commandElements[$i]
            if ($element -isnot [StringConstantExpressionAst] -or
                $element.StringConstantType -ne [StringConstantType]::BareWord -or
                $element.Value.StartsWith('-')) {
                break
        }
        $element.Value
    }) -join ';'

    $completions = @(switch ($command) {
        'feroxbuster' {
            [CompletionResult]::new('-w', 'w', [CompletionResultType]::ParameterName, 'Path to the wordlist')
            [CompletionResult]::new('--wordlist', 'wordlist', [CompletionResultType]::ParameterName, 'Path to the wordlist')
            [CompletionResult]::new('-u', 'u', [CompletionResultType]::ParameterName, 'The target URL(s) (required, unless --stdin used)')
            [CompletionResult]::new('--url', 'url', [CompletionResultType]::ParameterName, 'The target URL(s) (required, unless --stdin used)')
            [CompletionResult]::new('-t', 't', [CompletionResultType]::ParameterName, 'Number of concurrent threads (default: 50)')
            [CompletionResult]::new('--threads', 'threads', [CompletionResultType]::ParameterName, 'Number of concurrent threads (default: 50)')
            [CompletionResult]::new('-d', 'd', [CompletionResultType]::ParameterName, 'Maximum recursion depth, a depth of 0 is infinite recursion (default: 4)')
            [CompletionResult]::new('--depth', 'depth', [CompletionResultType]::ParameterName, 'Maximum recursion depth, a depth of 0 is infinite recursion (default: 4)')
            [CompletionResult]::new('-T', 'T', [CompletionResultType]::ParameterName, 'Number of seconds before a request times out (default: 7)')
            [CompletionResult]::new('--timeout', 'timeout', [CompletionResultType]::ParameterName, 'Number of seconds before a request times out (default: 7)')
            [CompletionResult]::new('-p', 'p', [CompletionResultType]::ParameterName, 'Proxy to use for requests (ex: http(s)://host:port, socks5(h)://host:port)')
            [CompletionResult]::new('--proxy', 'proxy', [CompletionResultType]::ParameterName, 'Proxy to use for requests (ex: http(s)://host:port, socks5(h)://host:port)')
            [CompletionResult]::new('-P', 'P', [CompletionResultType]::ParameterName, 'Send only unfiltered requests through a Replay Proxy, instead of all requests')
            [CompletionResult]::new('--replay-proxy', 'replay-proxy', [CompletionResultType]::ParameterName, 'Send only unfiltered requests through a Replay Proxy, instead of all requests')
            [CompletionResult]::new('-R', 'R', [CompletionResultType]::ParameterName, 'Status Codes to send through a Replay Proxy when found (default: --status-codes value)')
            [CompletionResult]::new('--replay-codes', 'replay-codes', [CompletionResultType]::ParameterName, 'Status Codes to send through a Replay Proxy when found (default: --status-codes value)')
            [CompletionResult]::new('-s', 's', [CompletionResultType]::ParameterName, 'Status Codes to include (allow list) (default: 200 204 301 302 307 308 401 403 405)')
            [CompletionResult]::new('--status-codes', 'status-codes', [CompletionResultType]::ParameterName, 'Status Codes to include (allow list) (default: 200 204 301 302 307 308 401 403 405)')
            [CompletionResult]::new('-o', 'o', [CompletionResultType]::ParameterName, 'Output file to write results to (use w/ --json for JSON entries)')
            [CompletionResult]::new('--output', 'output', [CompletionResultType]::ParameterName, 'Output file to write results to (use w/ --json for JSON entries)')
            [CompletionResult]::new('--resume-from', 'resume-from', [CompletionResultType]::ParameterName, 'State file from which to resume a partially complete scan (ex. --resume-from ferox-1606586780.state)')
            [CompletionResult]::new('--debug-log', 'debug-log', [CompletionResultType]::ParameterName, 'Output file to write log entries (use w/ --json for JSON entries)')
            [CompletionResult]::new('-a', 'a', [CompletionResultType]::ParameterName, 'Sets the User-Agent (default: feroxbuster/VERSION)')
            [CompletionResult]::new('--user-agent', 'user-agent', [CompletionResultType]::ParameterName, 'Sets the User-Agent (default: feroxbuster/VERSION)')
            [CompletionResult]::new('-x', 'x', [CompletionResultType]::ParameterName, 'File extension(s) to search for (ex: -x php -x pdf js)')
            [CompletionResult]::new('--extensions', 'extensions', [CompletionResultType]::ParameterName, 'File extension(s) to search for (ex: -x php -x pdf js)')
            [CompletionResult]::new('-H', 'H', [CompletionResultType]::ParameterName, 'Specify HTTP headers (ex: -H Header:val ''stuff: things'')')
            [CompletionResult]::new('--headers', 'headers', [CompletionResultType]::ParameterName, 'Specify HTTP headers (ex: -H Header:val ''stuff: things'')')
            [CompletionResult]::new('-Q', 'Q', [CompletionResultType]::ParameterName, 'Specify URL query parameters (ex: -Q token=stuff -Q secret=key)')
            [CompletionResult]::new('--query', 'query', [CompletionResultType]::ParameterName, 'Specify URL query parameters (ex: -Q token=stuff -Q secret=key)')
            [CompletionResult]::new('-S', 'S', [CompletionResultType]::ParameterName, 'Filter out messages of a particular size (ex: -S 5120 -S 4927,1970)')
            [CompletionResult]::new('--filter-size', 'filter-size', [CompletionResultType]::ParameterName, 'Filter out messages of a particular size (ex: -S 5120 -S 4927,1970)')
            [CompletionResult]::new('-X', 'X', [CompletionResultType]::ParameterName, 'Filter out messages via regular expression matching on the response''s body (ex: -X ''^ignore me$'')')
            [CompletionResult]::new('--filter-regex', 'filter-regex', [CompletionResultType]::ParameterName, 'Filter out messages via regular expression matching on the response''s body (ex: -X ''^ignore me$'')')
            [CompletionResult]::new('-W', 'W', [CompletionResultType]::ParameterName, 'Filter out messages of a particular word count (ex: -W 312 -W 91,82)')
            [CompletionResult]::new('--filter-words', 'filter-words', [CompletionResultType]::ParameterName, 'Filter out messages of a particular word count (ex: -W 312 -W 91,82)')
            [CompletionResult]::new('-N', 'N', [CompletionResultType]::ParameterName, 'Filter out messages of a particular line count (ex: -N 20 -N 31,30)')
            [CompletionResult]::new('--filter-lines', 'filter-lines', [CompletionResultType]::ParameterName, 'Filter out messages of a particular line count (ex: -N 20 -N 31,30)')
            [CompletionResult]::new('-C', 'C', [CompletionResultType]::ParameterName, 'Filter out status codes (deny list) (ex: -C 200 -C 401)')
            [CompletionResult]::new('--filter-status', 'filter-status', [CompletionResultType]::ParameterName, 'Filter out status codes (deny list) (ex: -C 200 -C 401)')
            [CompletionResult]::new('--filter-similar-to', 'filter-similar-to', [CompletionResultType]::ParameterName, 'Filter out pages that are similar to the given page (ex. --filter-similar-to http://site.xyz/soft404)')
            [CompletionResult]::new('-L', 'L', [CompletionResultType]::ParameterName, 'Limit total number of concurrent scans (default: 0, i.e. no limit)')
            [CompletionResult]::new('--scan-limit', 'scan-limit', [CompletionResultType]::ParameterName, 'Limit total number of concurrent scans (default: 0, i.e. no limit)')
            [CompletionResult]::new('--time-limit', 'time-limit', [CompletionResultType]::ParameterName, 'Limit total run time of all scans (ex: --time-limit 10m)')
            [CompletionResult]::new('-v', 'v', [CompletionResultType]::ParameterName, 'Increase verbosity level (use -vv or more for greater effect. [CAUTION] 4 -v''s is probably too much)')
            [CompletionResult]::new('--verbosity', 'verbosity', [CompletionResultType]::ParameterName, 'Increase verbosity level (use -vv or more for greater effect. [CAUTION] 4 -v''s is probably too much)')
            [CompletionResult]::new('-q', 'q', [CompletionResultType]::ParameterName, 'Only print URLs; Don''t print status codes, response size, running config, etc...')
            [CompletionResult]::new('--quiet', 'quiet', [CompletionResultType]::ParameterName, 'Only print URLs; Don''t print status codes, response size, running config, etc...')
            [CompletionResult]::new('--json', 'json', [CompletionResultType]::ParameterName, 'Emit JSON logs to --output and --debug-log instead of normal text')
            [CompletionResult]::new('-D', 'D', [CompletionResultType]::ParameterName, 'Don''t auto-filter wildcard responses')
            [CompletionResult]::new('--dont-filter', 'dont-filter', [CompletionResultType]::ParameterName, 'Don''t auto-filter wildcard responses')
            [CompletionResult]::new('-r', 'r', [CompletionResultType]::ParameterName, 'Follow redirects')
            [CompletionResult]::new('--redirects', 'redirects', [CompletionResultType]::ParameterName, 'Follow redirects')
            [CompletionResult]::new('-k', 'k', [CompletionResultType]::ParameterName, 'Disables TLS certificate validation')
            [CompletionResult]::new('--insecure', 'insecure', [CompletionResultType]::ParameterName, 'Disables TLS certificate validation')
            [CompletionResult]::new('-n', 'n', [CompletionResultType]::ParameterName, 'Do not scan recursively')
            [CompletionResult]::new('--no-recursion', 'no-recursion', [CompletionResultType]::ParameterName, 'Do not scan recursively')
            [CompletionResult]::new('-f', 'f', [CompletionResultType]::ParameterName, 'Append / to each request')
            [CompletionResult]::new('--add-slash', 'add-slash', [CompletionResultType]::ParameterName, 'Append / to each request')
            [CompletionResult]::new('--stdin', 'stdin', [CompletionResultType]::ParameterName, 'Read url(s) from STDIN')
            [CompletionResult]::new('-e', 'e', [CompletionResultType]::ParameterName, 'Extract links from response body (html, javascript, etc...); make new requests based on findings (default: false)')
            [CompletionResult]::new('--extract-links', 'extract-links', [CompletionResultType]::ParameterName, 'Extract links from response body (html, javascript, etc...); make new requests based on findings (default: false)')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Prints help information')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Prints help information')
            [CompletionResult]::new('-V', 'V', [CompletionResultType]::ParameterName, 'Prints version information')
            [CompletionResult]::new('--version', 'version', [CompletionResultType]::ParameterName, 'Prints version information')
            break
        }
    })

    $completions.Where{ $_.CompletionText -like "$wordToComplete*" } |
        Sort-Object -Property ListItemText
}
