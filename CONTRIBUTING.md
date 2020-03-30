# Contributing to Witnet

:tada: Thank you for joining the Witnet community and showing interest
in making your first contribution! :tada:

The following is a set of guidelines and helpful pointers for
contributing to Witnet. The keyword here is _guidelines_, not rules. As
such, use your best judgement and feel free to propose changes to even
this document.

## Code of conduct

Everyone participating in this project is governed by the
[Witnet Code of Conduct][code]. By participating, you are expected to
uphold this code as well.

## I just have a question

Please don't file an issue with questions.It's easier for you and for us
if you go directly to our [Discord server][discord] or
[Telegram group][telegram], since it will keep our repositories clean
and you will get a faster response.

## How can I contribute?

Find an area you can help with and do it. Open source is about
collaboration and open participation. Try to make your code look like
what already exists and submit a pull request on [GitHub].

The [list of issues][issues] is a good place to start, especially the
ones tagged as "[good first issue][first-issue]" or "help wanted" (but
don't let that stop you from looking at others). If you're looking for
additional ideas, try to search `TODO` comments for suggestions on minor
to major improvements. `grep` is your friend.

Pull requests adding more tests or documentation are rewarded with an immense amount of positive karma.

### Reporting bugs

This section guides you through submitting a bug report. This helps
contributors and maintainers understand your report, reproduce the
behavior, and in turn squash the bug.

Before submitting a bug report, please make sure that you've searched
through the issues and that there isn't already an issue describing the
same issue you are having.

### How do I submit a good bug report?

Bugs are tracked as [GitHub issues][issues].

Explain the problem and include additional details to help maintainers
reproduce the problem:

* Use a clear and descriptive title for the issue to identify the
  problem.
* Describe the exact steps which reproduce the problem in as many
  details as possible.
* Provide specific examples to demonstrate the steps. Include links to
  files or GitHub projects, or copy/pasteable snippets, which you use in
  those examples. If you're providing snippets in the issue, use
  Markdown code blocks.
* Describe the behavior you observed after following the steps and point
  out what exactly is the problem with that behavior.
* Explain which behavior you expected to see instead and why.
* Post a screenshot or a dump of the console when possible and suitable.
* If the problem wasn't triggered by a specific action, describe what
  you were doing before the problem happened and share more information
  using the guidelines below.

Provide more context by answering these questions:

* Did the problem start happening recently (e.g. after updating to a new
  version) or was this always a problem?
* If the problem started happening recently, can you reproduce the
  problem in an older version of Witnet-rust? What's the most recent
  version in which the problem doesn't happen?
* Can you reliably reproduce the issue? If not, provide details about
  how often the problem happens and under which conditions it normally
  happens.
* Are you running `witnet-rust` from a pre-compiled binary or from the
  source code?
* What's your operating system and version?

## Suggesting enhancements

This section guides you through submitting an enhancement suggestion,
including completely new features and minor improvements to existing
functionality. Following these guidelines helps maintainers and the
community understand your suggestion.

Before creating enhancement suggestions, please double check that there
is not already an existing feature suggestion for your feature, as you
might find out that you don't need to create one. When you are creating
an enhancement suggestion, please include as many details as possible.

### How Do I Submit A Good Enhancement Suggestion?

Enhancement suggestions are tracked as GitHub issues. Create an issue on
that repository and provide the following information:

* Use a clear and descriptive title for the issue to identify the
  suggestion.
* Provide a step-by-step description of the suggested enhancement in as
  many details as possible.
* Provide specific examples to demonstrate the steps. Include
  copy/pasteable snippets which you use in those examples, as Markdown
  code blocks.
* Describe the current behavior and explain which behavior you expected
  to see instead and why.
* Explain why this enhancement would be useful to most users and isn't
  something that can or should be implemented as a community package.

### Your First Code Contribution

Unsure where to begin contributing? You can start by looking through
these good first issue issues:

* [Good first issue][first-issue] - issues which should only require a
  few lines of code, and a test or two.

## Sending a Pull Request

### Commit messages convention

We use a [commit message convention][convention] to make our commit
history easier to understand for everyone and allow for automatic
generation of changelogs.

These are some examples of good commit messages:
    
```
feat(mining): use a random nonce as input in mint transactions

BREAKING CHANGE: former mint transactions containing no inputs will be rendered invalid 
```
```
refactor(config): make `config` actor return settings as `Option`s
```
```
docs: add RADON `FLOAT_TOSTRING` opcode

this operator converts any floating point number into a UTF8 string
```
```
chore(cargo): upgrade `actix` to version 0.8.1

fix #503
```

### PGP-signing your commits
 
All commits in the Witnet project repositories need to be signed by
their authors using PGP.

To configure your Git client to sign commits by default for a local repository, in Git versions 2.0.0 and above, run `git config commit.gpgsign true`.
To sign all commits by default in any local repository on your computer, run `git config --global commit.gpgsign true`.

To store your GPG key passphrase so you don't have to enter it every time you sign a commit, we recommend using the following tools:

- For Mac users, the [GPG Suite] allows you to store your GPG key passphrase in the Mac OS Keychain.
- For Windows users, the [Gpg4win] integrates with other Windows tools.

You can also manually configure [gpg-agent] to save your GPG key passphrase, but this doesn't integrate with Mac OS Keychain like ssh-agent and requires more setup.

If you have multiple keys or are attempting to sign commits or tags with
a key that doesn't match your committer identity, you should
[tell Git about your signing key][signing-key].


[code]: https://github.com/witnet/witnet-rust/blob/master/CODE_OF_CONDUCT.md
[issues]: https://github.com/witnet/witnet-rust/issues
[discord]: https://discord.gg/FDPPv7H
[telegram]: https://t.me/witnetio
[GitHub]: https://github.com/witnet/witnet-rust
[first-issue]: https://github.com/witnet/witnet-rust/labels/good%20first%20issue%20%F0%9F%91%8B
[convention]: https://www.conventionalcommits.org/en/v1.0.0-beta.2/
[GPG Suite]: https://gpgtools.org/
[Gpg4win]: https://www.gpg4win.org/
[gpg-agent]: http://linux.die.net/man/1/gpg-agent
[signing-key]: https://help.github.com/en/articles/telling-git-about-your-signing-key
