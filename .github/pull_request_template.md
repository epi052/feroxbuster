# Landing a Pull Request (PR)

Long form explanations of most of the items below can be found in the [CONTRIBUTING](https://github.com/epi052/feroxbuster/blob/master/CONTRIBUTING.md) guide.

## Branching checklist
- [ ] There is an issue associated with your PR (bug, feature, etc.. if not, create one)
- [ ] Your PR description references the associated issue (i.e. fixes #123)
- [ ] Code is in its own branch
- [ ] Branch name is related to the PR contents
- [ ] PR targets master

## Static analysis checks
- [ ] All rust files are formatted using `cargo fmt`
- [ ] All `clippy` checks pass when running `cargo clippy --all-targets --all-features -- -D warnings -A clippy::deref_addrof`
- [ ] All existing tests pass

## Documentation
- [ ] New code is documented using [doc comments](https://doc.rust-lang.org/stable/rust-by-example/meta/doc.html)
- [ ] Documentation about your PR is included in the README, as needed

## Additional Tests
- [ ] New code is unit tested
- [ ] New code is integration tested, as needed
- [ ] New tests pass
