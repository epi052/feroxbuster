
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
                $element.Value.StartsWith('-') -or
                $element.Value -eq $wordToComplete) {
                break
        }
        $element.Value
    }) -join ';'

    $completions = @(switch ($command) {
        'feroxbuster' {
            [CompletionResult]::new('-u', '-u', [CompletionResultType]::ParameterName, 'The target URL (required, unless [--stdin || --resume-from || --request-file] used)')
            [CompletionResult]::new('--url', '--url', [CompletionResultType]::ParameterName, 'The target URL (required, unless [--stdin || --resume-from || --request-file] used)')
            [CompletionResult]::new('--resume-from', '--resume-from', [CompletionResultType]::ParameterName, 'State file from which to resume a partially complete scan (ex. --resume-from ferox-1606586780.state)')
            [CompletionResult]::new('--request-file', '--request-file', [CompletionResultType]::ParameterName, 'Raw HTTP request file to use as a template for all requests')
            [CompletionResult]::new('-p', '-p', [CompletionResultType]::ParameterName, 'Proxy to use for requests (ex: http(s)://host:port, socks5(h)://host:port)')
            [CompletionResult]::new('--proxy', '--proxy', [CompletionResultType]::ParameterName, 'Proxy to use for requests (ex: http(s)://host:port, socks5(h)://host:port)')
            [CompletionResult]::new('-P', '-P ', [CompletionResultType]::ParameterName, 'Send only unfiltered requests through a Replay Proxy, instead of all requests')
            [CompletionResult]::new('--replay-proxy', '--replay-proxy', [CompletionResultType]::ParameterName, 'Send only unfiltered requests through a Replay Proxy, instead of all requests')
            [CompletionResult]::new('-R', '-R ', [CompletionResultType]::ParameterName, 'Status Codes to send through a Replay Proxy when found (default: --status-codes value)')
            [CompletionResult]::new('--replay-codes', '--replay-codes', [CompletionResultType]::ParameterName, 'Status Codes to send through a Replay Proxy when found (default: --status-codes value)')
            [CompletionResult]::new('-a', '-a', [CompletionResultType]::ParameterName, 'Sets the User-Agent (default: feroxbuster/2.11.0)')
            [CompletionResult]::new('--user-agent', '--user-agent', [CompletionResultType]::ParameterName, 'Sets the User-Agent (default: feroxbuster/2.11.0)')
            [CompletionResult]::new('-x', '-x', [CompletionResultType]::ParameterName, 'File extension(s) to search for (ex: -x php -x pdf js); reads values (newline-separated) from file if input starts with an @ (ex: @ext.txt)')
            [CompletionResult]::new('--extensions', '--extensions', [CompletionResultType]::ParameterName, 'File extension(s) to search for (ex: -x php -x pdf js); reads values (newline-separated) from file if input starts with an @ (ex: @ext.txt)')
            [CompletionResult]::new('-m', '-m', [CompletionResultType]::ParameterName, 'Which HTTP request method(s) should be sent (default: GET)')
            [CompletionResult]::new('--methods', '--methods', [CompletionResultType]::ParameterName, 'Which HTTP request method(s) should be sent (default: GET)')
            [CompletionResult]::new('--data', '--data', [CompletionResultType]::ParameterName, 'Request''s Body; can read data from a file if input starts with an @ (ex: @post.bin)')
            [CompletionResult]::new('-H', '-H ', [CompletionResultType]::ParameterName, 'Specify HTTP headers to be used in each request (ex: -H Header:val -H ''stuff: things'')')
            [CompletionResult]::new('--headers', '--headers', [CompletionResultType]::ParameterName, 'Specify HTTP headers to be used in each request (ex: -H Header:val -H ''stuff: things'')')
            [CompletionResult]::new('-b', '-b', [CompletionResultType]::ParameterName, 'Specify HTTP cookies to be used in each request (ex: -b stuff=things)')
            [CompletionResult]::new('--cookies', '--cookies', [CompletionResultType]::ParameterName, 'Specify HTTP cookies to be used in each request (ex: -b stuff=things)')
            [CompletionResult]::new('-Q', '-Q ', [CompletionResultType]::ParameterName, 'Request''s URL query parameters (ex: -Q token=stuff -Q secret=key)')
            [CompletionResult]::new('--query', '--query', [CompletionResultType]::ParameterName, 'Request''s URL query parameters (ex: -Q token=stuff -Q secret=key)')
            [CompletionResult]::new('--protocol', '--protocol', [CompletionResultType]::ParameterName, 'Specify the protocol to use when targeting via --request-file or --url with domain only (default: https)')
            [CompletionResult]::new('--dont-scan', '--dont-scan', [CompletionResultType]::ParameterName, 'URL(s) or Regex Pattern(s) to exclude from recursion/scans')
            [CompletionResult]::new('-S', '-S ', [CompletionResultType]::ParameterName, 'Filter out messages of a particular size (ex: -S 5120 -S 4927,1970)')
            [CompletionResult]::new('--filter-size', '--filter-size', [CompletionResultType]::ParameterName, 'Filter out messages of a particular size (ex: -S 5120 -S 4927,1970)')
            [CompletionResult]::new('-X', '-X ', [CompletionResultType]::ParameterName, 'Filter out messages via regular expression matching on the response''s body/headers (ex: -X ''^ignore me$'')')
            [CompletionResult]::new('--filter-regex', '--filter-regex', [CompletionResultType]::ParameterName, 'Filter out messages via regular expression matching on the response''s body/headers (ex: -X ''^ignore me$'')')
            [CompletionResult]::new('-W', '-W ', [CompletionResultType]::ParameterName, 'Filter out messages of a particular word count (ex: -W 312 -W 91,82)')
            [CompletionResult]::new('--filter-words', '--filter-words', [CompletionResultType]::ParameterName, 'Filter out messages of a particular word count (ex: -W 312 -W 91,82)')
            [CompletionResult]::new('-N', '-N ', [CompletionResultType]::ParameterName, 'Filter out messages of a particular line count (ex: -N 20 -N 31,30)')
            [CompletionResult]::new('--filter-lines', '--filter-lines', [CompletionResultType]::ParameterName, 'Filter out messages of a particular line count (ex: -N 20 -N 31,30)')
            [CompletionResult]::new('-C', '-C ', [CompletionResultType]::ParameterName, 'Filter out status codes (deny list) (ex: -C 200 -C 401)')
            [CompletionResult]::new('--filter-status', '--filter-status', [CompletionResultType]::ParameterName, 'Filter out status codes (deny list) (ex: -C 200 -C 401)')
            [CompletionResult]::new('--filter-similar-to', '--filter-similar-to', [CompletionResultType]::ParameterName, 'Filter out pages that are similar to the given page (ex. --filter-similar-to http://site.xyz/soft404)')
            [CompletionResult]::new('-s', '-s', [CompletionResultType]::ParameterName, 'Status Codes to include (allow list) (default: All Status Codes)')
            [CompletionResult]::new('--status-codes', '--status-codes', [CompletionResultType]::ParameterName, 'Status Codes to include (allow list) (default: All Status Codes)')
            [CompletionResult]::new('-T', '-T ', [CompletionResultType]::ParameterName, 'Number of seconds before a client''s request times out (default: 7)')
            [CompletionResult]::new('--timeout', '--timeout', [CompletionResultType]::ParameterName, 'Number of seconds before a client''s request times out (default: 7)')
            [CompletionResult]::new('--server-certs', '--server-certs', [CompletionResultType]::ParameterName, 'Add custom root certificate(s) for servers with unknown certificates')
            [CompletionResult]::new('--client-cert', '--client-cert', [CompletionResultType]::ParameterName, 'Add a PEM encoded certificate for mutual authentication (mTLS)')
            [CompletionResult]::new('--client-key', '--client-key', [CompletionResultType]::ParameterName, 'Add a PEM encoded private key for mutual authentication (mTLS)')
            [CompletionResult]::new('-t', '-t', [CompletionResultType]::ParameterName, 'Number of concurrent threads (default: 50)')
            [CompletionResult]::new('--threads', '--threads', [CompletionResultType]::ParameterName, 'Number of concurrent threads (default: 50)')
            [CompletionResult]::new('-d', '-d', [CompletionResultType]::ParameterName, 'Maximum recursion depth, a depth of 0 is infinite recursion (default: 4)')
            [CompletionResult]::new('--depth', '--depth', [CompletionResultType]::ParameterName, 'Maximum recursion depth, a depth of 0 is infinite recursion (default: 4)')
            [CompletionResult]::new('-L', '-L ', [CompletionResultType]::ParameterName, 'Limit total number of concurrent scans (default: 0, i.e. no limit)')
            [CompletionResult]::new('--scan-limit', '--scan-limit', [CompletionResultType]::ParameterName, 'Limit total number of concurrent scans (default: 0, i.e. no limit)')
            [CompletionResult]::new('--parallel', '--parallel', [CompletionResultType]::ParameterName, 'Run parallel feroxbuster instances (one child process per url passed via stdin)')
            [CompletionResult]::new('--rate-limit', '--rate-limit', [CompletionResultType]::ParameterName, 'Limit number of requests per second (per directory) (default: 0, i.e. no limit)')
            [CompletionResult]::new('--time-limit', '--time-limit', [CompletionResultType]::ParameterName, 'Limit total run time of all scans (ex: --time-limit 10m)')
            [CompletionResult]::new('-w', '-w', [CompletionResultType]::ParameterName, 'Path or URL of the wordlist')
            [CompletionResult]::new('--wordlist', '--wordlist', [CompletionResultType]::ParameterName, 'Path or URL of the wordlist')
            [CompletionResult]::new('-B', '-B ', [CompletionResultType]::ParameterName, 'Automatically request likely backup extensions for "found" urls (default: ~, .bak, .bak2, .old, .1)')
            [CompletionResult]::new('--collect-backups', '--collect-backups', [CompletionResultType]::ParameterName, 'Automatically request likely backup extensions for "found" urls (default: ~, .bak, .bak2, .old, .1)')
            [CompletionResult]::new('-I', '-I ', [CompletionResultType]::ParameterName, 'File extension(s) to Ignore while collecting extensions (only used with --collect-extensions)')
            [CompletionResult]::new('--dont-collect', '--dont-collect', [CompletionResultType]::ParameterName, 'File extension(s) to Ignore while collecting extensions (only used with --collect-extensions)')
            [CompletionResult]::new('-o', '-o', [CompletionResultType]::ParameterName, 'Output file to write results to (use w/ --json for JSON entries)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Output file to write results to (use w/ --json for JSON entries)')
            [CompletionResult]::new('--debug-log', '--debug-log', [CompletionResultType]::ParameterName, 'Output file to write log entries (use w/ --json for JSON entries)')
            [CompletionResult]::new('--limit-bars', '--limit-bars', [CompletionResultType]::ParameterName, 'Number of directory scan bars to show at any given time (default: no limit)')
            [CompletionResult]::new('--stdin', '--stdin', [CompletionResultType]::ParameterName, 'Read url(s) from STDIN')
            [CompletionResult]::new('--burp', '--burp', [CompletionResultType]::ParameterName, 'Set --proxy to http://127.0.0.1:8080 and set --insecure to true')
            [CompletionResult]::new('--burp-replay', '--burp-replay', [CompletionResultType]::ParameterName, 'Set --replay-proxy to http://127.0.0.1:8080 and set --insecure to true')
            [CompletionResult]::new('--smart', '--smart', [CompletionResultType]::ParameterName, 'Set --auto-tune, --collect-words, and --collect-backups to true')
            [CompletionResult]::new('--thorough', '--thorough', [CompletionResultType]::ParameterName, 'Use the same settings as --smart and set --collect-extensions and --scan-dir-listings to true')
            [CompletionResult]::new('-A', '-A ', [CompletionResultType]::ParameterName, 'Use a random User-Agent')
            [CompletionResult]::new('--random-agent', '--random-agent', [CompletionResultType]::ParameterName, 'Use a random User-Agent')
            [CompletionResult]::new('-f', '-f', [CompletionResultType]::ParameterName, 'Append / to each request''s URL')
            [CompletionResult]::new('--add-slash', '--add-slash', [CompletionResultType]::ParameterName, 'Append / to each request''s URL')
            [CompletionResult]::new('-r', '-r', [CompletionResultType]::ParameterName, 'Allow client to follow redirects')
            [CompletionResult]::new('--redirects', '--redirects', [CompletionResultType]::ParameterName, 'Allow client to follow redirects')
            [CompletionResult]::new('-k', '-k', [CompletionResultType]::ParameterName, 'Disables TLS certificate validation in the client')
            [CompletionResult]::new('--insecure', '--insecure', [CompletionResultType]::ParameterName, 'Disables TLS certificate validation in the client')
            [CompletionResult]::new('-n', '-n', [CompletionResultType]::ParameterName, 'Do not scan recursively')
            [CompletionResult]::new('--no-recursion', '--no-recursion', [CompletionResultType]::ParameterName, 'Do not scan recursively')
            [CompletionResult]::new('--force-recursion', '--force-recursion', [CompletionResultType]::ParameterName, 'Force recursion attempts on all ''found'' endpoints (still respects recursion depth)')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Extract links from response body (html, javascript, etc...); make new requests based on findings (default: true)')
            [CompletionResult]::new('--extract-links', '--extract-links', [CompletionResultType]::ParameterName, 'Extract links from response body (html, javascript, etc...); make new requests based on findings (default: true)')
            [CompletionResult]::new('--dont-extract-links', '--dont-extract-links', [CompletionResultType]::ParameterName, 'Don''t extract links from response body (html, javascript, etc...)')
            [CompletionResult]::new('--auto-tune', '--auto-tune', [CompletionResultType]::ParameterName, 'Automatically lower scan rate when an excessive amount of errors are encountered')
            [CompletionResult]::new('--auto-bail', '--auto-bail', [CompletionResultType]::ParameterName, 'Automatically stop scanning when an excessive amount of errors are encountered')
            [CompletionResult]::new('-D', '-D ', [CompletionResultType]::ParameterName, 'Don''t auto-filter wildcard responses')
            [CompletionResult]::new('--dont-filter', '--dont-filter', [CompletionResultType]::ParameterName, 'Don''t auto-filter wildcard responses')
            [CompletionResult]::new('-E', '-E ', [CompletionResultType]::ParameterName, 'Automatically discover extensions and add them to --extensions (unless they''re in --dont-collect)')
            [CompletionResult]::new('--collect-extensions', '--collect-extensions', [CompletionResultType]::ParameterName, 'Automatically discover extensions and add them to --extensions (unless they''re in --dont-collect)')
            [CompletionResult]::new('-g', '-g', [CompletionResultType]::ParameterName, 'Automatically discover important words from within responses and add them to the wordlist')
            [CompletionResult]::new('--collect-words', '--collect-words', [CompletionResultType]::ParameterName, 'Automatically discover important words from within responses and add them to the wordlist')
            [CompletionResult]::new('--scan-dir-listings', '--scan-dir-listings', [CompletionResultType]::ParameterName, 'Force scans to recurse into directory listings')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Increase verbosity level (use -vv or more for greater effect. [CAUTION] 4 -v''s is probably too much)')
            [CompletionResult]::new('--verbosity', '--verbosity', [CompletionResultType]::ParameterName, 'Increase verbosity level (use -vv or more for greater effect. [CAUTION] 4 -v''s is probably too much)')
            [CompletionResult]::new('--silent', '--silent', [CompletionResultType]::ParameterName, 'Only print URLs (or JSON w/ --json) + turn off logging (good for piping a list of urls to other commands)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Hide progress bars and banner (good for tmux windows w/ notifications)')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Hide progress bars and banner (good for tmux windows w/ notifications)')
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Emit JSON logs to --output and --debug-log instead of normal text')
            [CompletionResult]::new('--no-state', '--no-state', [CompletionResultType]::ParameterName, 'Disable state output file (*.state)')
            [CompletionResult]::new('-U', '-U ', [CompletionResultType]::ParameterName, 'Update feroxbuster to the latest version')
            [CompletionResult]::new('--update', '--update', [CompletionResultType]::ParameterName, 'Update feroxbuster to the latest version')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
    })

    $completions.Where{ $_.CompletionText -like "$wordToComplete*" } |
        Sort-Object -Property ListItemText
}
