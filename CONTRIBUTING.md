# Contributor's guide

<!-- this guide is a modified version of the guide that I already modified which was based on the one used by the awesome guys that wrote cmd2 -->

First of all, thank you for contributing! Please follow these steps to contribute:

1. Find an issue that needs assistance by searching for the [Help Wanted](https://github.com/epi052/feroxbuster/issues?q=is%3Aissue+is%3Aopen+label%3A%22help+wanted%22) tag
2. Let us know you're working on it by posting a comment on the issue
3. Follow the [Contribution guidelines](#contribution-guidelines) to start working on the issue

Remember to feel free to ask for help by leaving a comment within the Issue.

Working on your first pull request? You can learn how from this *free* series
[How to Contribute to an Open Source Project on GitHub](https://egghead.io/series/how-to-contribute-to-an-open-source-project-on-github).

###### If you've found a bug that is not on the board, [follow these steps](README.md#found-a-bug).

---

## Contribution guidelines

- [Prerequisites](#prerequisites)
- [Forking the project](#forking-the-project)
- [Creating a branch](#creating-a-branch)
- [Setting up for recon-pipeline development](#setting-up-for-recon-pipeline-development)
- [Making changes](#making-changes)
- [Static code analysis](#static-code-analysis)
- [Running the test suite](#running-the-test-suite)
- [Squashing your commits](#squashing-your-commits)
- [Creating a pull request](#creating-a-pull-request)
- [How we review and merge pull requests](#how-we-review-and-merge-pull-requests)
- [Next steps](#next-steps)
- [Other resources](#other-resources)
- [Advice](#advice)

### Forking the project

#### Setting up your system

1. Install your favorite `git` client
2. Create a parent projects directory on your system. For this guide, it will be assumed that it is `~/projects`.

#### Forking feroxbuster

1. Go to the top-level feroxbuster repository: <https://github.com/epi052/feroxbuster>
2. Click the "Fork" button in the upper right hand corner of the interface
([more details here](https://help.github.com/articles/fork-a-repo/))
3. After the repository has been forked, you will be taken to your copy of the feroxbuster repo at `your_username/feroxbuster`

#### Cloning your fork

1. Open a terminal / command line / Bash shell in your projects directory (_e.g.: `~/projects/`_)
2. Clone your fork of feroxbuster, making sure to replace `your_username` with your GitHub username. This will download the
entire feroxbuster repo to your projects directory.

```sh
$ git clone https://github.com/your_username/feroxbuster.git
```

#### Set up your upstream

1. Change directory to the new feroxbuster directory (`cd feroxbuster`)
2. Add a remote to the official feroxbuster repo:

```sh
$ git remote add upstream https://github.com/epi052/feroxbuster.git
```

Now you have a local copy of the feroxbuster repo!

#### Maintaining your fork

Now that you have a copy of your fork, there is work you will need to do to keep it current.

##### **Rebasing from upstream**

Do this prior to every time you create a branch for a PR:

1. Make sure you are on the `main` branch

  > ```sh
  > $ git status
  > On branch main
  > Your branch is up-to-date with 'origin/main'.
  > ```

  > If your aren't on `main`, resolve outstanding files and commits and checkout the `main` branch

  > ```sh
  > $ git checkout main
  > ```

2. Do a pull with rebase against `upstream`

  > ```sh
  > $ git pull --rebase upstream main
  > ```

  > This will pull down all of the changes to the official main branch, without making an additional commit in your local repo.

3. (_Optional_) Force push your updated main branch to your GitHub fork

  > ```sh
  > $ git push origin main --force
  > ```

  > This will overwrite the main branch of your fork.

### Creating a branch

Before you start working, you will need to create a separate branch specific to the issue or feature you're working on.
You will push your work to this branch.

#### Naming your branch

Name the branch something like `23-xxx` where `xxx` is a short description of the changes or feature
you are attempting to add and `23` corresponds to the Issue you're working on.

#### Adding your branch

To create a branch on your local machine (and switch to this branch):

```sh
$ git checkout -b [name_of_your_new_branch]
```

and to push to GitHub:

```sh
$ git push origin [name_of_your_new_branch]
```

##### If you need more help with branching, take a look at _[this](https://github.com/Kunena/Kunena-Forum/wiki/Create-a-new-branch-with-git-and-manage-branches)_.

### Setting up for feroxbuster development
For doing feroxbuster development, all you really need is `rust` installed on your system (I'll leave the choice of IDE to you, but VS Code and JetBrains both have very nice rust plugins).

#### Install rustup

The primary way that folks install Rust is through a tool called Rustup, which is a Rust installer and version management tool.

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup update
```

After running the two commands above, you should be able to run `cargo`.

```shell script
$> cargo --version
cargo 1.45.0 (744bd1fbb 2020-06-15)
```

### Making changes

It's your time to shine!

#### How to find code in the feroxbuster codebase to fix/edit

The feroxbuster project directory structure is pretty simple and straightforward.  All
actual code for feroxbuster is located underneath the `src` directory. Integration tests are in the
`tests` directory.  There are various other files in the root directory, but these are
primarily related to continuous integration and release deployment.

### Static code analysis

feroxbuster uses the [`clippy`](https://rust-lang.github.io/rust-clippy/) code linter.

The command that will ultimately be used in the CI pipeline for linting is `cargo clippy --all-targets --all-features -- -D warnings -A clippy::mutex-atomic`.

Before submitting a Pull Request, the above command should be run. Please do not ignore any linting errors in code you write or modify, as they are meant to **help** by ensuring a clean and simple code base.

### Running the test suite
When you're ready to share your code, run the test suite:
```sh
$ cd ~/projects/feroxbuster
$ cargo test
```
and ensure all tests pass.

Test coverage can be checked using [grcov](https://github.com/mozilla/grcov).  Installation and execution are summarized below.

```sh
cargo install grcov
rustup install nightly
rustup default nightly
export CARGO_INCREMENTAL=0
export RUSTFLAGS="-Zprofile -Ccodegen-units=1 -Copt-level=0 -Clink-dead-code -Coverflow-checks=off -Zpanic_abort_tests -Cpanic=abort"
export RUSTDOCFLAGS="-Cpanic=abort"
cargo build
cargo test
grcov ./target/debug/ -s . -t html --llvm --branch --ignore-not-existing -o ./target/debug/coverage/
firefox target/debug/coverage/index.html
```

### Squashing your commits

When you make a pull request, it is preferable for all of your changes to be in one commit.  Github has made it very
simple to squash commits now as it's [available through the web interface](https://stackoverflow.com/a/43858707) at
pull request submission time.

### Creating a pull request

#### What is a pull request?

A pull request (PR) is a method of submitting proposed changes to the feroxbuster
repo (or any repo, for that matter). You will make changes to copies of the
files which make up feroxbuster in a personal fork, then apply to have them
accepted by the feroxbuster team.

#### Need help?

GitHub has a good guide on how to contribute to open source [here](https://opensource.guide/how-to-contribute/).

##### Editing via your local fork

1.  Perform the maintenance step of rebasing `main`
2.  Ensure you're on the `main` branch using `git status`:

```sh
$ git status
On branch main
Your branch is up-to-date with 'origin/main'.

nothing to commit, working directory clean
```

1.  If you're not on main or your working directory is not clean, resolve
    any outstanding files/commits and checkout main `git checkout main`
2.  Create a branch off of `main` with git: `git checkout -B
    branch/name-here`
3.  Edit your file(s) locally with the editor of your choice
4.  Check your `git status` to see unstaged files
5.  Add your edited files: `git add path/to/filename.ext` You can also do: `git
    add .` to add all unstaged files. Take care, though, because you can
    accidentally add files you don't want added. Review your `git status` first.
6.  Commit your edits: `git commit -m "Brief description of commit"`.
7.  Squash your commits, if there are more than one
8.  Push your commits to your GitHub Fork: `git push -u origin branch/name-here`
9.  Once the edits have been committed, you will be prompted to create a pull
    request on your fork's GitHub page
10.  By default, all pull requests should be against the `main` branch
11.  Submit a pull request from your branch to feroxbuster's `main` branch
12.  The title (also called the subject) of your PR should be descriptive of your
    changes and succinctly indicate what is being fixed
    -   Examples: `Add test cases for Unicode support`; `Correct typo in overview documentation`
13.  In the body of your PR include a more detailed summary of the changes you
    made and why
    -   If the PR is meant to fix an existing bug/issue, then, at the end of
        your PR's description, append the keyword `closes` and #xxxx (where xxxx
        is the issue number). Example: `closes #1337`. This tells GitHub to
        close the existing issue if the PR is merged.
14.  Creating the PR causes our continuous integration (CI) systems to automatically run all of the
    unit tests on all supported OSes. You should watch your PR to make sure that all unit tests pass.
15.  If any unit tests fail, you should look at the details and fix the failures. You can then push
    the fix to the same branch in your fork. The PR will automatically get updated and the CI system
    will automatically run all of the unit tests again.

### How we review and merge pull requests

1. If your changes can merge without conflicts and all unit tests pass, then your pull request (PR) will have a big
green checkbox which says something like "All Checks Passed" next to it. If this is not the case, there will be a
link you can click on to get details regarding what the problem is.  It is your responsibility to make sure all unit
tests are passing.  Generally a Maintainer will not QA a pull request unless it can merge without conflicts and all
unit tests pass.

2. If a Maintainer reviews a pull request and confirms that the new code does what it is supposed to do without
seeming to introduce any new bugs, and doesn't present any backward compatibility issues, they will merge the pull request.

### Next steps

#### If your PR is accepted

Once your PR is accepted, you may delete the branch you created to submit it.
This keeps your working fork clean.

You can do this with a press of a button on the GitHub PR interface. You can
delete the local copy of the branch with: `git branch -D branch/to-delete-name`

#### If your PR is rejected

Don't worry! You will receive solid feedback from the Maintainers as to
why it was rejected and what changes are needed.

Many pull requests, especially first pull requests, require correction or
updating.

If you have a local copy of the repo, you can make the requested changes and
amend your commit with: `git commit --amend` This will update your existing
commit. When you push it to your fork you will need to do a force push to
overwrite your old commit: `git push --force`

Be sure to post in the PR conversation that you have made the requested changes.

### Other resources

-   [Searching for your issue on GitHub](https://help.github.com/articles/searching-issues/)
-   [Creating a new GitHub issue](https://help.github.com/articles/creating-an-issue/)

### Advice

Here is some advice regarding what makes a good pull request (PR) from our perspective:
- Multiple smaller PRs divided by topic are better than a single large PR containing a bunch of unrelated changes
- Good unit/functional tests are very important
- Accurate documentation is also important
- It's best to create a dedicated branch for a PR, use it only for that PR, and delete it once the PR has been merged
- It's good if the branch name is related to the PR contents, even if it's just "fix123" or "add_more_tests"
- Code coverage of the unit tests matters, so try not to decrease it
- Think twice before adding dependencies to third-party libraries because it could affect a lot of users

## Acknowledgement
Thanks to the awesome guys at [cmd2](https://github.com/python-cmd2/cmd2) for their fantastic `CONTRIBUTING` file from
which we have borrowed heavily.
