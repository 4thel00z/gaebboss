# CLI reference

`gaebboss --help` and `gaebboss <command> --help` print the options. Exit
codes: 0 on success; 1 when a file cannot be read or written, when a jq
program fails while running, or when `validate` finds errors; 2 for an
invalid jq program or a bad argument.
