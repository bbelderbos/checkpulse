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
