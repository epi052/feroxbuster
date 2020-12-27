_feroxbuster() {
    local i cur prev opts cmds
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"
    cmd=""
    opts=""

    for i in ${COMP_WORDS[@]}
    do
        case "${i}" in
            feroxbuster)
                cmd="feroxbuster"
                ;;
            
            *)
                ;;
        esac
    done

    case "${cmd}" in
        feroxbuster)
            opts=" -v -q -D -r -k -n -f -e -h -V -w -u -t -d -T -p -P -R -s -o -a -x -H -Q -S -X -W -N -C -L  --verbosity --quiet --json --dont-filter --redirects --insecure --no-recursion --add-slash --stdin --extract-links --help --version --wordlist --url --threads --depth --timeout --proxy --replay-proxy --replay-codes --status-codes --output --resume-from --debug-log --user-agent --extensions --headers --query --filter-size --filter-regex --filter-words --filter-lines --filter-status --filter-similar-to --scan-limit --time-limit  "
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 1 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                
                --wordlist)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                    -w)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --url)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                    -u)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --threads)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                    -t)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --depth)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                    -d)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --timeout)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                    -T)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --proxy)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                    -p)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --replay-proxy)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                    -P)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --replay-codes)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                    -R)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --status-codes)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                    -s)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                    -o)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --resume-from)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --debug-log)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --user-agent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                    -a)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --extensions)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                    -x)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --headers)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                    -H)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --query)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                    -Q)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --filter-size)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                    -S)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --filter-regex)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                    -X)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --filter-words)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                    -W)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --filter-lines)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                    -N)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --filter-status)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                    -C)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --filter-similar-to)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --scan-limit)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                    -L)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --time-limit)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        
    esac
}

complete -F _feroxbuster -o bashdefault -o default feroxbuster
