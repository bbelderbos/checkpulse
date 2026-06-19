default:
    @just --list

# run the test suite
test:
    cargo test

# run tests with a coverage summary in the terminal
cov:
    cargo llvm-cov --summary-only

# run tests and open an HTML coverage report
cov-html:
    cargo llvm-cov --html --open

# download the deployed SQLite DB to ./checkpulse-prod.db (gitignored)
pull-db:
    fly ssh sftp get /data/checkpulse.db ./checkpulse-prod.db

# custom-event totals (no NAME), or per-article breakdown for NAME, over the last DAYS days
# examples: just events  ·  just events cohort-python-agentic  ·  just events cta-bottom 7 checkpulse-prod.db
events name="" days="30" db="checkpulse-prod.db":
    #!/usr/bin/env bash
    set -euo pipefail
    cutoff=$(( $(date +%s) - {{days}} * 86400 ))
    if [ -z "{{name}}" ]; then
      sqlite3 -header -column "{{db}}" \
        "SELECT name, COUNT(*) AS events FROM events
         WHERE name IS NOT NULL AND ts >= $cutoff
         GROUP BY name ORDER BY events DESC;"
    else
      sqlite3 -header -column "{{db}}" \
        "SELECT path, COUNT(*) AS clicks FROM events
         WHERE name = '{{name}}' AND ts >= $cutoff
         GROUP BY path ORDER BY clicks DESC;"
    fi
