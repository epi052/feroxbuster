
@('feroxbuster', './feroxbuster') | %{
    Register-ArgumentCompleter -Native -CommandName $_ -ScriptBlock {
        param($wordToComplete, $commandAst, $cursorPosition)

        $command = '_feroxbuster'
        $commandAst.CommandElements |
            Select-Object -Skip 1 |
            %{
                switch ($_.ToString()) {

                    'feroxbuster' {
                        $command += '_feroxbuster'
                        break
                    }

                    default { 
                        break
                    }
                }
            }

        $completions = @()

        switch ($command) {

            '_feroxbuster' {
                $completions = @('-v', '-q', '-D', '-r', '-k', '-n', '-f', '-e', '-h', '-V', '-w', '-u', '-t', '-d', '-T', '-p', '-P', '-R', '-s', '-o', '-a', '-x', '-H', '-Q', '-S', '-X', '-W', '-N', '-C', '-L', '--verbosity', '--quiet', '--json', '--dont-filter', '--redirects', '--insecure', '--no-recursion', '--add-slash', '--stdin', '--extract-links', '--help', '--version', '--wordlist', '--url', '--threads', '--depth', '--timeout', '--proxy', '--replay-proxy', '--replay-codes', '--status-codes', '--output', '--resume-from', '--debug-log', '--user-agent', '--extensions', '--headers', '--query', '--filter-size', '--filter-regex', '--filter-words', '--filter-lines', '--filter-status', '--scan-limit')
            }

        }

        $completions |
            ?{ $_ -like "$wordToComplete*" } |
            Sort-Object |
            %{ New-Object System.Management.Automation.CompletionResult $_, $_, 'ParameterValue', $_ }
    }
}
