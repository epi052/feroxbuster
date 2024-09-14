# Landing a Pull Request (PR)

Long form explanations of most of the items below can be found in the [CONTRIBUTING](https://github.com/epi052/feroxbuster/blob/master/CONTRIBUTING.md) guide.

## Branching checklist
- [ ] There is an issue associated with your PR (bug, feature, etc.. if not, create one)
- [ ] Your PR description references the associated issue (i.e. fixes #123456)
- [ ] Code is in its own branch
- [ ] Branch name is related to the PR contents
- [ ] PR targets main

## Static analysis checks
- [ ] All rust files are formatted using `cargo fmt`
- [ ] All `clippy` checks pass when running `cargo clippy --all-targets --all-features -- -D warnings -A clippy::mutex-atomic`
- [ ] All existing tests pass

## Documentation
- [ ] New code is documented using [doc comments](https://doc.rust-lang.org/stable/rust-by-example/meta/doc.html)
- [ ] Documentation about your PR is included in the `docs`, as needed. The docs live in a [separate repository](https://epi052.github.io/feroxbuster-docs/docs/). Update the appropriate pages at the links below.
  - [ ] update [example config file section](https://epi052.github.io/feroxbuster-docs/docs/configuration/ferox-config-toml/)
  - [ ] update [help output section](https://epi052.github.io/feroxbuster-docs/docs/configuration/command-line/)
  - [ ] add an [example](https://epi052.github.io/feroxbuster-docs/docs/examples/)

## Additional Tests
- [ ] New code is unit tested
- [ ] New code is integration tested, as needed
- [ ] New tests pass
